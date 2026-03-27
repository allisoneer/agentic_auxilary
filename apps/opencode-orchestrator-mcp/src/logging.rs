use agentic_logging::LogWriter;
use agentic_logging::ToolCallRecord;
use chrono::Utc;
use std::path::PathBuf;

pub const OPENCODE_ORCHESTRATOR_LOG_DIR: &str = "OPENCODE_ORCHESTRATOR_LOG_DIR";

pub fn resolve_logs_dir() -> Option<PathBuf> {
    if let Ok(value) = std::env::var(OPENCODE_ORCHESTRATOR_LOG_DIR) {
        let value = value.trim();
        if !value.is_empty() {
            return Some(PathBuf::from(value));
        }
    }

    thoughts_tool::documents::active_logs_dir().ok()
}

pub fn append_record_best_effort(record: &ToolCallRecord) {
    let Some(dir) = resolve_logs_dir() else {
        return;
    };

    let writer = LogWriter::new(dir);
    if let Err(error) = writer.append_jsonl(record) {
        tracing::warn!(error = %error, "Failed to append orchestrator JSONL log");
    }
}

pub fn write_markdown_best_effort(
    completed_at: agentic_logging::chrono::DateTime<Utc>,
    call_id: &str,
    content: &str,
) -> Option<String> {
    let dir = resolve_logs_dir()?;

    let writer = LogWriter::new(dir);
    match writer.write_markdown_response(completed_at, call_id, content) {
        Ok(filename) if !filename.is_empty() => Some(filename),
        Ok(_) => None,
        Err(error) => {
            tracing::warn!(error = %error, "Failed to write orchestrator markdown log");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_logging::CallTimer;
    use agentic_logging::ToolCallRecord;
    use std::io::Read;
    use std::sync::Mutex;
    use std::sync::OnceLock;

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    fn sample_record() -> ToolCallRecord {
        let timer = CallTimer::start();
        let (completed_at, duration_ms) = timer.finish();
        ToolCallRecord {
            call_id: timer.call_id,
            server: "opencode-orchestrator-mcp".into(),
            tool: "run".into(),
            started_at: timer.started_at,
            completed_at,
            duration_ms,
            request: serde_json::json!({"message": "hello"}),
            response_file: None,
            success: true,
            error: None,
            model: None,
            token_usage: None,
            summary: None,
        }
    }

    #[test]
    fn env_log_dir_writes_jsonl() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvVarGuard {
            key: OPENCODE_ORCHESTRATOR_LOG_DIR,
            previous: std::env::var_os(OPENCODE_ORCHESTRATOR_LOG_DIR),
        };
        let tmp = tempfile::tempdir().unwrap();

        // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_LOG_DIR, tmp.path()) };

        let record = sample_record();
        append_record_best_effort(&record);

        let bucket = format!("tool_logs_{}", record.completed_at.format("%Y-%m-%d"));
        let path = tmp.path().join(format!("{bucket}.jsonl"));
        assert!(path.exists());

        let mut content = String::new();
        std::fs::File::open(path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert!(content.contains("opencode-orchestrator-mcp"));
    }

    #[test]
    fn invalid_log_dir_is_swallowed() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _env = EnvVarGuard {
            key: OPENCODE_ORCHESTRATOR_LOG_DIR,
            previous: std::env::var_os(OPENCODE_ORCHESTRATOR_LOG_DIR),
        };
        let tmp = tempfile::tempdir().unwrap();
        let invalid_path = tmp.path().join("not-a-directory");
        std::fs::write(&invalid_path, "file").unwrap();

        // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_LOG_DIR, &invalid_path) };

        append_record_best_effort(&sample_record());
        assert!(invalid_path.is_file());
    }
}
