//! Logging utilities for pr_comments.
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
            server: "pr_comments".to_string(),
            tool: tool.to_string(),
        }
    }

    /// Finish the logging context and append the JSONL record.
    ///
    /// If `completed_at` is provided, it will be used for the JSONL record to ensure
    /// consistent day-bucket placement. Otherwise, a fresh timestamp is captured.
    ///
    /// This is best-effort: errors are logged via tracing but do not fail the call.
    #[allow(clippy::too_many_arguments)]
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
        assert_eq!(ctx.server, "pr_comments");
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
}
