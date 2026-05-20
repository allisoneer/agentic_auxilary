//! Shared tool-call logging for `thoughts_tool`.
//!
//! This module provides logging helpers used by both MCP and agentic-tools
//! wrappers to ensure consistent logging behavior.

use crate::documents::active_logs_dir;
use agentic_logging::CallTimer;
use agentic_logging::LogWriter;
use agentic_logging::ToolCallRecord;

fn classify_failure_kind(success: bool, error: Option<&str>) -> Option<String> {
    if success {
        return None;
    }

    let error = error.unwrap_or_default().to_ascii_lowercase();
    if error.contains("timed out") || error.contains("timeout") {
        Some("timeout".to_string())
    } else if error.contains("cancelled") || error.contains("canceled") {
        Some("cancelled".to_string())
    } else {
        Some("error".to_string())
    }
}

/// Log a tool call. Behavior identical to MCP.
///
/// If logging is unavailable (e.g., no active branch), this function
/// returns silently without panicking or affecting the caller.
pub fn log_tool_call(
    timer: &CallTimer,
    tool: &str,
    request: serde_json::Value,
    success: bool,
    error: Option<String>,
    summary: Option<serde_json::Value>,
) {
    let writer = match active_logs_dir() {
        Ok(dir) => LogWriter::new(dir),
        Err(_) => return, // Logging unavailable (e.g., branch lockout)
    };

    let (completed_at, duration_ms) = timer.finish();
    let record = ToolCallRecord {
        call_id: timer.call_id.clone(),
        server: "thoughts_tool".into(),
        tool: tool.into(),
        started_at: timer.started_at,
        completed_at,
        duration_ms,
        request,
        response_file: None,
        success,
        failure_kind: classify_failure_kind(success, error.as_deref()),
        error,
        model: None,
        token_usage: None,
        summary,
    };

    if let Err(e) = writer.append_jsonl(&record) {
        tracing::warn!("Failed to append JSONL log: {}", e);
    }
}
