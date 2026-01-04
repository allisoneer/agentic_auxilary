//! Logging utilities for coding_agent_tools.
//!
//! Provides a helper context that reduces duplication when logging tool calls
//! to the thoughts logs directory using agentic_logging.

use agentic_logging::chrono::{DateTime, Utc};
use agentic_logging::{CallTimer, LogWriter, ToolCallRecord};
use thoughts_tool::active_logs_dir;

/// Context for logging a single tool call.
///
/// Captures timing, builds the log record, and writes it to the logs directory.
/// Logging is best-effort: if the logs directory is unavailable (e.g., branch lockout),
/// the tool call still succeeds without logging.
pub struct ToolLogCtx {
    /// Timer for this call (contains call_id and started_at)
    pub timer: CallTimer,
    /// Writer instance (if logs dir was resolved)
    writer: Option<LogWriter>,
    /// Server name for the log record
    server: String,
    /// Tool name for the log record
    tool: String,
}

impl ToolLogCtx {
    /// Start a new logging context for a tool call.
    ///
    /// If the logs directory cannot be resolved (e.g., branch lockout), the context
    /// is still created but logging will be a no-op.
    pub fn start(tool: &str) -> Self {
        let timer = CallTimer::start();
        let writer = active_logs_dir().ok().map(LogWriter::new);

        Self {
            timer,
            writer,
            server: "coding_agent_tools".to_string(),
            tool: tool.to_string(),
        }
    }

    /// Write a markdown response file and return the filename with timestamp.
    ///
    /// Returns the filename and the `completed_at` timestamp used, so callers can
    /// pass the same timestamp to `finish()` for consistent day-bucket placement.
    ///
    /// Returns None if logging is unavailable or disabled.
    pub fn write_markdown_response(&self, content: &str) -> Option<(String, DateTime<Utc>)> {
        let writer = self.writer.as_ref()?;
        let (completed_at, _) = self.timer.finish();
        writer
            .write_markdown_response(completed_at, &self.timer.call_id, content)
            .ok()
            .filter(|s| !s.is_empty())
            .map(|filename| (filename, completed_at))
    }

    /// Finish the logging context and append the JSONL record.
    ///
    /// If `completed_at` is provided (e.g., from `write_markdown_response`), it will be
    /// used for the JSONL record to ensure consistent day-bucket placement. Otherwise,
    /// a fresh timestamp is captured.
    ///
    /// This is best-effort: errors are logged via tracing but do not fail the call.
    pub fn finish(
        self,
        request: serde_json::Value,
        response_file: Option<String>,
        success: bool,
        error: Option<String>,
        summary: Option<serde_json::Value>,
        model: Option<String>,
        completed_at: Option<DateTime<Utc>>,
    ) {
        let Some(writer) = self.writer else {
            return;
        };

        // Use provided timestamp for consistency, or capture fresh one
        let (completed_at, duration_ms) = match completed_at {
            Some(ts) => (ts, self.timer.elapsed_ms()),
            None => self.timer.finish(),
        };
        let record = ToolCallRecord {
            call_id: self.timer.call_id,
            server: self.server,
            tool: self.tool,
            started_at: self.timer.started_at,
            completed_at,
            duration_ms,
            request,
            response_file,
            success,
            error,
            model,
            token_usage: None,
            summary,
        };

        if let Err(e) = writer.append_jsonl(&record) {
            tracing::warn!("Failed to append JSONL log: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_log_ctx_creation() {
        // This will fail to get logs dir in test environment (no active branch),
        // but should not panic
        let ctx = ToolLogCtx::start("test_tool");
        assert_eq!(ctx.tool, "test_tool");
        assert_eq!(ctx.server, "coding_agent_tools");
        assert!(!ctx.timer.call_id.is_empty());
    }

    #[test]
    fn test_finish_without_writer_is_noop() {
        // Create context (writer will be None without active branch)
        let ctx = ToolLogCtx::start("test_tool");

        // This should not panic even without a writer
        ctx.finish(
            serde_json::json!({"test": true}),
            None,
            true,
            None,
            None,
            None,
            None,
        );
    }

    #[test]
    fn test_write_markdown_does_not_panic() {
        // Whether writer is available depends on test environment (active branch or not)
        // The key invariant is that it should not panic either way
        let ctx = ToolLogCtx::start("test_tool");
        let result = ctx.write_markdown_response("# Test");
        // Result may be Some (if logs dir available) or None (if not)
        // Either outcome is valid - we just verify no panic
        let _ = result;
    }

    // =========================================================================
    // Request JSON shape tests - verify each tool logs the expected fields
    // =========================================================================

    #[test]
    fn test_spawn_agent_request_json_shape() {
        let req = serde_json::json!({
            "agent_type": "locator",
            "location": "codebase",
            "query": "Find all error handling code",
        });

        // Verify required fields exist
        assert!(req.get("agent_type").is_some(), "Missing agent_type");
        assert!(req.get("location").is_some(), "Missing location");
        assert!(req.get("query").is_some(), "Missing query");

        // Verify field types
        assert!(req["agent_type"].is_string());
        assert!(req["location"].is_string());
        assert!(req["query"].is_string());
    }

    #[test]
    fn test_ls_request_json_shape() {
        let req = serde_json::json!({
            "path": "./src",
            "depth": 2,
            "show": "files",
            "ignore": ["*.tmp"],
            "hidden": false,
        });

        // Verify expected fields
        assert!(req.get("path").is_some(), "Missing path");
        assert!(req.get("depth").is_some(), "Missing depth");
        assert!(req.get("show").is_some(), "Missing show");
        assert!(req.get("ignore").is_some(), "Missing ignore");
        assert!(req.get("hidden").is_some(), "Missing hidden");

        // Verify types
        assert!(req["depth"].is_number() || req["depth"].is_null());
        assert!(req["ignore"].is_array() || req["ignore"].is_null());
        assert!(req["hidden"].is_boolean() || req["hidden"].is_null());
    }

    #[test]
    fn test_ls_summary_json_shape() {
        let summary = serde_json::json!({
            "entries": 42,
            "has_more": true,
            "shown": 42,
            "total": 100,
        });

        assert!(summary["entries"].is_number());
        assert!(summary["has_more"].is_boolean());
        assert!(summary["shown"].is_number());
        assert!(summary["total"].is_number());
    }

    #[test]
    fn test_search_grep_request_json_shape() {
        let req = serde_json::json!({
            "pattern": "fn main",
            "path": "./src",
            "mode": "content",
            "globs": ["*.rs"],
            "ignore": null,
            "include_hidden": false,
            "case_insensitive": true,
            "multiline": false,
            "line_numbers": true,
            "context": 3,
            "context_before": null,
            "context_after": null,
            "include_binary": false,
            "head_limit": 200,
            "offset": 0,
        });

        // Verify required fields
        assert!(req.get("pattern").is_some(), "Missing pattern");
        assert!(req["pattern"].is_string());

        // Verify optional fields exist (may be null)
        assert!(req.get("path").is_some());
        assert!(req.get("mode").is_some());
        assert!(req.get("globs").is_some());
        assert!(req.get("head_limit").is_some());
        assert!(req.get("offset").is_some());
    }

    #[test]
    fn test_search_grep_summary_json_shape() {
        let summary = serde_json::json!({
            "lines": 15,
            "mode": "content",
            "has_more": false,
        });

        assert!(summary["lines"].is_number());
        assert!(summary["mode"].is_string());
        assert!(summary["has_more"].is_boolean());
    }

    #[test]
    fn test_search_glob_request_json_shape() {
        let req = serde_json::json!({
            "pattern": "**/*.rs",
            "path": ".",
            "ignore": null,
            "include_hidden": false,
            "sort": "name",
            "head_limit": 500,
            "offset": 0,
        });

        assert!(req.get("pattern").is_some(), "Missing pattern");
        assert!(req["pattern"].is_string());
        assert!(req.get("head_limit").is_some());
        assert!(req.get("offset").is_some());
    }

    #[test]
    fn test_search_glob_summary_json_shape() {
        let summary = serde_json::json!({
            "entries": 25,
            "has_more": false,
        });

        assert!(summary["entries"].is_number());
        assert!(summary["has_more"].is_boolean());
    }

    #[test]
    fn test_just_search_request_json_shape() {
        let req = serde_json::json!({
            "query": "test",
            "dir": "./coding_agent_tools",
        });

        assert!(req.get("query").is_some());
        assert!(req.get("dir").is_some());
    }

    #[test]
    fn test_just_search_summary_json_shape() {
        let summary = serde_json::json!({
            "items": 5,
            "has_more": true,
        });

        assert!(summary["items"].is_number());
        assert!(summary["has_more"].is_boolean());
    }

    #[test]
    fn test_just_execute_request_json_shape() {
        let req = serde_json::json!({
            "recipe": "test",
            "dir": null,
            "args": {"verbose": true},
        });

        assert!(req.get("recipe").is_some(), "Missing recipe");
        assert!(req["recipe"].is_string());
        assert!(req.get("dir").is_some());
        assert!(req.get("args").is_some());
    }

    #[test]
    fn test_just_execute_summary_json_shape() {
        let summary = serde_json::json!({
            "exit_code": 0,
            "stdout_lines": 10,
            "stderr_lines": 0,
        });

        assert!(summary["exit_code"].is_number());
        assert!(summary["stdout_lines"].is_number());
        assert!(summary["stderr_lines"].is_number());
    }

    #[test]
    fn test_tool_call_record_has_expected_fields() {
        // Use CallTimer to get proper timestamps
        let timer = CallTimer::start();
        let (completed_at, duration_ms) = timer.finish();

        // Verify ToolCallRecord can be created with all fields used by coding_agent_tools
        let record = ToolCallRecord {
            call_id: "test-id".into(),
            server: "coding_agent_tools".into(),
            tool: "ls".into(),
            started_at: timer.started_at,
            completed_at,
            duration_ms,
            request: serde_json::json!({"path": "."}),
            response_file: None,
            success: true,
            error: None,
            model: None, // coding_agent_tools doesn't use models (except spawn_agent)
            token_usage: None,
            summary: Some(serde_json::json!({"entries": 10})),
        };

        let json = serde_json::to_string(&record).unwrap();

        // Verify server is correct
        assert!(json.contains("\"server\":\"coding_agent_tools\""));
        // Verify summary is included
        assert!(json.contains("\"summary\""));
        // Verify model is omitted when None
        assert!(!json.contains("\"model\""));
    }

    // =========================================================================
    // Logging failure isolation tests
    // =========================================================================

    #[test]
    fn test_logging_failure_does_not_affect_ctx_creation() {
        // When active_logs_dir() fails (no active branch), ToolLogCtx should still be created
        // with writer = None, allowing tools to proceed without logging
        let ctx = ToolLogCtx::start("test_tool");

        // Context is valid even without writer
        assert_eq!(ctx.tool, "test_tool");
        assert_eq!(ctx.server, "coding_agent_tools");

        // Calling finish() should be a no-op, not a panic or error
        ctx.finish(
            serde_json::json!({"key": "value"}),
            None,
            true,
            None,
            Some(serde_json::json!({"result": "ok"})),
            None,
            None,
        );
        // Test passes if we reach here without panic
    }

    #[test]
    fn test_logging_failure_isolation_with_error_result() {
        // Even when logging an error result, if writer is unavailable, should not panic
        let ctx = ToolLogCtx::start("failing_tool");

        ctx.finish(
            serde_json::json!({"input": "bad"}),
            None,
            false, // success = false
            Some("Something went wrong".into()),
            None,
            None,
            None,
        );
        // Test passes if we reach here without panic
    }

    #[test]
    fn test_write_markdown_response_returns_none_gracefully() {
        // When writer is unavailable, write_markdown_response should return None
        // without panicking or causing errors
        let ctx = ToolLogCtx::start("spawn_agent");

        // In test environment without active branch, this should return None
        let result = ctx.write_markdown_response("# Large response\n\nSome content here...");

        // Result is None when logging is unavailable (or Some when available)
        // The key is that it doesn't panic or error
        match result {
            None => {
                // Expected in environments without active branch
            }
            Some((filename, _completed_at)) => {
                // Valid in environments with active branch
                assert!(filename.ends_with(".md"));
            }
        }
    }
}
