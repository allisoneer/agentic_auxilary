//! Shared tool-call logging for thoughts_tool.
//!
//! This module provides logging helpers used by both MCP and agentic-tools
//! wrappers to ensure consistent logging behavior.

use crate::documents::active_logs_dir;
use agentic_logging::{CallTimer, LogWriter, ToolCallRecord};

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
        error,
        model: None,
        token_usage: None,
        summary,
    };

    if let Err(e) = writer.append_jsonl(&record) {
        tracing::warn!("Failed to append JSONL log: {}", e);
    }
}
