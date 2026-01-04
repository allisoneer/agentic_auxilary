//! Centralized JSONL logging infrastructure for agentic tools.
//!
//! This crate provides utilities for logging tool calls to JSONL files with:
//! - Atomic writes with file locking
//! - Optional markdown response files for large outputs
//! - Daily bucket organization
//! - Disable via `AGENTIC_LOGGING_DISABLED=1` environment variable

use atomicwrites::{AtomicFile, OverwriteBehavior};
use chrono::{DateTime, Utc};

// Re-export chrono types for downstream crates
pub use chrono;
use fd_lock::RwLock;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during logging operations.
#[derive(Error, Debug)]
pub enum LogError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Atomic write error: {0}")]
    AtomicWrite(String),
}

impl<E: std::fmt::Display> From<atomicwrites::Error<E>> for LogError {
    fn from(e: atomicwrites::Error<E>) -> Self {
        LogError::AtomicWrite(e.to_string())
    }
}

/// Token usage information for API calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt: u32,
    pub completion: u32,
    pub total: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
}

/// A single tool call record for JSONL logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// Unique identifier for this call
    pub call_id: String,
    /// Server name (e.g., "gpt5_reasoner", "coding_agent_tools", "thoughts_tool")
    pub server: String,
    /// Tool name (e.g., "plan", "reasoning", "spawn_agent", "ls")
    pub tool: String,
    /// When the call started
    pub started_at: DateTime<Utc>,
    /// When the call completed
    pub completed_at: DateTime<Utc>,
    /// Duration in milliseconds
    pub duration_ms: u128,
    /// Request parameters (serialized as JSON)
    pub request: serde_json::Value,
    /// Path to markdown response file, if one was written
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_file: Option<String>,
    /// Whether the call succeeded
    pub success: bool,
    /// Error message if the call failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Model used for the call (e.g., "openai/gpt-5")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Token usage if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
    /// Summary data for compact tools (e.g., {"entries": 10, "has_more": true})
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<serde_json::Value>,
}

/// Check if logging is disabled via environment variable.
pub fn logging_disabled() -> bool {
    match std::env::var("AGENTIC_LOGGING_DISABLED") {
        Ok(v) => matches!(v.as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => false,
    }
}

/// Timer utility for measuring call duration and generating call IDs.
pub struct CallTimer {
    /// Unique call identifier
    pub call_id: String,
    /// When the call started (UTC)
    pub started_at: DateTime<Utc>,
    /// Instant for measuring elapsed time
    start_instant: std::time::Instant,
}

impl CallTimer {
    /// Start a new timer with a fresh call ID.
    pub fn start() -> Self {
        Self {
            call_id: Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            start_instant: std::time::Instant::now(),
        }
    }

    /// Finish the timer and return the completion time and duration.
    pub fn finish(&self) -> (DateTime<Utc>, u128) {
        let completed_at = Utc::now();
        let duration_ms = self.start_instant.elapsed().as_millis();
        (completed_at, duration_ms)
    }

    /// Return the elapsed duration in milliseconds without capturing a new timestamp.
    ///
    /// Useful when you need to reuse a previously captured `completed_at` timestamp
    /// for consistent day-bucket placement.
    pub fn elapsed_ms(&self) -> u128 {
        self.start_instant.elapsed().as_millis()
    }
}

/// Writer for JSONL log files and markdown response files.
pub struct LogWriter {
    base_logs_dir: PathBuf,
}

impl LogWriter {
    /// Create a new log writer with the given base logs directory.
    pub fn new(base_logs_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_logs_dir: base_logs_dir.into(),
        }
    }

    /// Generate the day bucket name from a timestamp.
    fn day_bucket_name(date: DateTime<Utc>) -> String {
        date.format("tool_logs_%Y-%m-%d").to_string()
    }

    /// Ensure the JSONL file and markdown directory exist for a day bucket.
    pub fn ensure_day_dirs(&self, day_bucket: &str) -> Result<(PathBuf, PathBuf), LogError> {
        let jsonl = self.base_logs_dir.join(format!("{day_bucket}.jsonl"));
        let md_dir = self.base_logs_dir.join(day_bucket);
        std::fs::create_dir_all(&self.base_logs_dir)?;
        std::fs::create_dir_all(&md_dir)?;
        Ok((jsonl, md_dir))
    }

    /// Write a markdown response file and return its filename.
    ///
    /// Returns an empty string if logging is disabled.
    pub fn write_markdown_response(
        &self,
        completed_at: DateTime<Utc>,
        call_id: &str,
        content: &str,
    ) -> Result<String, LogError> {
        if logging_disabled() {
            return Ok(String::new());
        }
        let bucket = Self::day_bucket_name(completed_at);
        let (_jsonl, md_dir) = self.ensure_day_dirs(&bucket)?;
        let filename = format!("{call_id}.md");
        let target = md_dir.join(&filename);
        let af = AtomicFile::new(&target, OverwriteBehavior::AllowOverwrite);
        af.write(|f| f.write_all(content.as_bytes()))?;
        Ok(filename)
    }

    /// Append a tool call record to the JSONL log file.
    ///
    /// Uses file locking to prevent concurrent write corruption.
    /// Returns Ok(()) if logging is disabled.
    pub fn append_jsonl(&self, record: &ToolCallRecord) -> Result<(), LogError> {
        if logging_disabled() {
            return Ok(());
        }
        let bucket = Self::day_bucket_name(record.completed_at);
        let (jsonl_path, _md_dir) = self.ensure_day_dirs(&bucket)?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&jsonl_path)?;
        let mut lock = RwLock::new(file);
        let mut guard = lock.write()?;
        serde_json::to_writer(&mut *guard, record)?;
        guard.write_all(b"\n")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io::Read;

    #[test]
    fn test_call_timer_generates_uuid() {
        let timer = CallTimer::start();
        assert!(!timer.call_id.is_empty());
        assert!(Uuid::parse_str(&timer.call_id).is_ok());
    }

    #[test]
    fn test_call_timer_measures_duration() {
        let timer = CallTimer::start();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let (completed_at, duration_ms) = timer.finish();
        assert!(duration_ms >= 10);
        assert!(completed_at >= timer.started_at);
    }

    #[test]
    #[serial]
    fn test_logging_disabled_env_var() {
        // SAFETY: serial_test ensures no concurrent env access
        unsafe {
            // Test with various values
            std::env::set_var("AGENTIC_LOGGING_DISABLED", "1");
            assert!(logging_disabled());

            std::env::set_var("AGENTIC_LOGGING_DISABLED", "true");
            assert!(logging_disabled());

            std::env::set_var("AGENTIC_LOGGING_DISABLED", "yes");
            assert!(logging_disabled());

            std::env::set_var("AGENTIC_LOGGING_DISABLED", "on");
            assert!(logging_disabled());

            std::env::set_var("AGENTIC_LOGGING_DISABLED", "0");
            assert!(!logging_disabled());

            std::env::set_var("AGENTIC_LOGGING_DISABLED", "false");
            assert!(!logging_disabled());

            std::env::remove_var("AGENTIC_LOGGING_DISABLED");
            assert!(!logging_disabled());
        }
    }

    #[test]
    fn test_day_bucket_name_format() {
        let date = DateTime::parse_from_rfc3339("2025-03-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(LogWriter::day_bucket_name(date), "tool_logs_2025-03-15");
    }

    #[test]
    #[serial]
    fn test_jsonl_append_creates_file() {
        let temp = tempfile::tempdir().unwrap();
        let writer = LogWriter::new(temp.path());

        let timer = CallTimer::start();
        let (completed_at, duration_ms) = timer.finish();

        let record = ToolCallRecord {
            call_id: timer.call_id.clone(),
            server: "test_server".into(),
            tool: "test_tool".into(),
            started_at: timer.started_at,
            completed_at,
            duration_ms,
            request: serde_json::json!({"param": "value"}),
            response_file: None,
            success: true,
            error: None,
            model: None,
            token_usage: None,
            summary: None,
        };

        writer.append_jsonl(&record).unwrap();

        // Check file was created
        let bucket = LogWriter::day_bucket_name(completed_at);
        let jsonl_path = temp.path().join(format!("{bucket}.jsonl"));
        assert!(jsonl_path.exists());

        // Check content
        let mut content = String::new();
        std::fs::File::open(&jsonl_path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert!(content.contains("test_server"));
        assert!(content.contains("test_tool"));
        assert!(content.ends_with('\n'));
    }

    #[test]
    #[serial]
    fn test_jsonl_append_multiple_lines() {
        let temp = tempfile::tempdir().unwrap();
        let writer = LogWriter::new(temp.path());

        // Write two records
        for i in 0..2 {
            let timer = CallTimer::start();
            let (completed_at, duration_ms) = timer.finish();
            let record = ToolCallRecord {
                call_id: timer.call_id,
                server: "test".into(),
                tool: format!("tool_{i}"),
                started_at: timer.started_at,
                completed_at,
                duration_ms,
                request: serde_json::json!({}),
                response_file: None,
                success: true,
                error: None,
                model: None,
                token_usage: None,
                summary: None,
            };
            writer.append_jsonl(&record).unwrap();
        }

        // Verify two lines
        let bucket = LogWriter::day_bucket_name(Utc::now());
        let jsonl_path = temp.path().join(format!("{bucket}.jsonl"));
        let content = std::fs::read_to_string(&jsonl_path).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("tool_0"));
        assert!(lines[1].contains("tool_1"));
    }

    #[test]
    #[serial]
    fn test_markdown_response_file() {
        let temp = tempfile::tempdir().unwrap();
        let writer = LogWriter::new(temp.path());

        let timer = CallTimer::start();
        let (completed_at, _) = timer.finish();

        let content = "# Test Response\n\nThis is markdown content.";
        let filename = writer
            .write_markdown_response(completed_at, &timer.call_id, content)
            .unwrap();

        assert_eq!(filename, format!("{}.md", timer.call_id));

        // Verify file content
        let bucket = LogWriter::day_bucket_name(completed_at);
        let md_path = temp.path().join(&bucket).join(&filename);
        let read_content = std::fs::read_to_string(&md_path).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    #[serial]
    fn test_disabled_logging_skips_writes() {
        // SAFETY: serial_test ensures no concurrent env access
        unsafe {
            std::env::set_var("AGENTIC_LOGGING_DISABLED", "1");
        }

        let temp = tempfile::tempdir().unwrap();
        let writer = LogWriter::new(temp.path());

        let timer = CallTimer::start();
        let (completed_at, duration_ms) = timer.finish();

        // JSONL should be no-op
        let record = ToolCallRecord {
            call_id: timer.call_id.clone(),
            server: "test".into(),
            tool: "test".into(),
            started_at: timer.started_at,
            completed_at,
            duration_ms,
            request: serde_json::json!({}),
            response_file: None,
            success: true,
            error: None,
            model: None,
            token_usage: None,
            summary: None,
        };
        writer.append_jsonl(&record).unwrap();

        // Markdown should return empty string
        let filename = writer
            .write_markdown_response(completed_at, &timer.call_id, "content")
            .unwrap();
        assert!(filename.is_empty());

        // No files should be created
        let entries: Vec<_> = std::fs::read_dir(temp.path()).unwrap().collect();
        assert!(entries.is_empty());

        // SAFETY: Tests run single-threaded with --test-threads=1 or are isolated
        unsafe {
            std::env::remove_var("AGENTIC_LOGGING_DISABLED");
        }
    }

    #[test]
    fn test_token_usage_serialization() {
        let usage = TokenUsage {
            prompt: 100,
            completion: 50,
            total: 150,
            reasoning_tokens: Some(7),
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("\"prompt\":100"));
        assert!(json.contains("\"completion\":50"));
        assert!(json.contains("\"total\":150"));
        assert!(json.contains("\"reasoning_tokens\":7"));

        let usage_none = TokenUsage {
            prompt: 1,
            completion: 2,
            total: 3,
            reasoning_tokens: None,
        };
        let json_none = serde_json::to_string(&usage_none).unwrap();
        assert!(!json_none.contains("reasoning_tokens"));
    }

    #[test]
    fn test_tool_call_record_optional_fields_omitted() {
        let timer = CallTimer::start();
        let (completed_at, duration_ms) = timer.finish();

        let record = ToolCallRecord {
            call_id: timer.call_id,
            server: "test".into(),
            tool: "test".into(),
            started_at: timer.started_at,
            completed_at,
            duration_ms,
            request: serde_json::json!({}),
            response_file: None,
            success: true,
            error: None,
            model: None,
            token_usage: None,
            summary: None,
        };

        let json = serde_json::to_string(&record).unwrap();
        // Optional None fields should not appear
        assert!(!json.contains("response_file"));
        assert!(!json.contains("error"));
        assert!(!json.contains("model"));
        assert!(!json.contains("token_usage"));
        assert!(!json.contains("summary"));
    }
}
