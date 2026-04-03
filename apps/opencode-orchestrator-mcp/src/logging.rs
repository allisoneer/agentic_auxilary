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
    use serial_test::serial;
    use std::io::Read;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
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
    #[serial(env)]
    fn env_log_dir_writes_jsonl() {
        let _env = EnvVarGuard {
            key: OPENCODE_ORCHESTRATOR_LOG_DIR,
            previous: std::env::var_os(OPENCODE_ORCHESTRATOR_LOG_DIR),
        };
        let tmp = tempfile::tempdir().unwrap();

        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_LOG_DIR, tmp.path()) };

        let record = sample_record();
        append_record_best_effort(&record);

        // Filename format: tool_logs_YYYY-MM-DD_{session_id}.jsonl
        let date_prefix = format!("tool_logs_{}", record.completed_at.format("%Y-%m-%d"));
        let jsonl_files: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with(&date_prefix) && name.ends_with(".jsonl")
            })
            .collect();

        assert_eq!(
            jsonl_files.len(),
            1,
            "Expected one JSONL file with today's date"
        );
        let path = jsonl_files[0].path();

        let mut content = String::new();
        std::fs::File::open(path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert!(content.contains("opencode-orchestrator-mcp"));
    }

    #[test]
    #[serial(env)]
    fn invalid_log_dir_is_swallowed() {
        let _env = EnvVarGuard {
            key: OPENCODE_ORCHESTRATOR_LOG_DIR,
            previous: std::env::var_os(OPENCODE_ORCHESTRATOR_LOG_DIR),
        };
        let tmp = tempfile::tempdir().unwrap();
        let invalid_path = tmp.path().join("not-a-directory");
        std::fs::write(&invalid_path, "file").unwrap();

        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_LOG_DIR, &invalid_path) };

        append_record_best_effort(&sample_record());
        assert!(invalid_path.is_file());
    }
}
