//! CLI runner fallback support.
//!
//! This module provides functionality to wrap `opencode run --format json`.

use crate::error::{OpencodeError, Result};
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Options for running the CLI.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Output format (default: "json").
    pub format: Option<String>,
    /// Attach to existing server URL.
    pub attach: Option<String>,
    /// Continue an existing session.
    pub continue_session: bool,
    /// Session ID to use/continue.
    pub session: Option<String>,
    /// Files to include.
    pub file: Vec<String>,
    /// Share the session.
    pub share: bool,
    /// Model to use (provider/model format).
    pub model: Option<String>,
    /// Agent to use.
    pub agent: Option<String>,
    /// Session title.
    pub title: Option<String>,
    /// Port to use.
    pub port: Option<u16>,
    /// Command to execute.
    pub command: Option<String>,
    /// Directory to run in.
    pub directory: Option<std::path::PathBuf>,
    /// Path to opencode binary.
    pub binary: String,
}

impl RunOptions {
    /// Create new RunOptions with defaults.
    pub fn new() -> Self {
        Self {
            binary: "opencode".into(),
            format: Some("json".into()),
            ..Default::default()
        }
    }

    /// Set output format.
    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    /// Set attach URL.
    pub fn attach(mut self, url: impl Into<String>) -> Self {
        self.attach = Some(url.into());
        self
    }

    /// Set continue session flag.
    pub fn continue_session(mut self, cont: bool) -> Self {
        self.continue_session = cont;
        self
    }

    /// Set session ID.
    pub fn session(mut self, id: impl Into<String>) -> Self {
        self.session = Some(id.into());
        self
    }

    /// Add a file.
    pub fn file(mut self, path: impl Into<String>) -> Self {
        self.file.push(path.into());
        self
    }

    /// Set share flag.
    pub fn share(mut self, share: bool) -> Self {
        self.share = share;
        self
    }

    /// Set model.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set agent.
    pub fn agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }

    /// Set title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set port.
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set command.
    pub fn command(mut self, cmd: impl Into<String>) -> Self {
        self.command = Some(cmd.into());
        self
    }

    /// Set directory.
    pub fn directory(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.directory = Some(dir.into());
        self
    }

    /// Set binary path.
    pub fn binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }
}

/// An event from the CLI runner (NDJSON format).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliEvent {
    /// Event type.
    pub r#type: String,
    /// Timestamp.
    #[serde(default)]
    pub timestamp: Option<i64>,
    /// Session ID.
    #[serde(rename = "sessionID", default)]
    pub session_id: Option<String>,
    /// Additional event data.
    #[serde(flatten)]
    pub data: serde_json::Value,
}

impl CliEvent {
    /// Check if this is a text event.
    pub fn is_text(&self) -> bool {
        self.r#type == "text"
    }

    /// Check if this is a step_start event.
    pub fn is_step_start(&self) -> bool {
        self.r#type == "step_start"
    }

    /// Check if this is a step_finish event.
    pub fn is_step_finish(&self) -> bool {
        self.r#type == "step_finish"
    }

    /// Check if this is an error event.
    pub fn is_error(&self) -> bool {
        self.r#type == "error"
    }

    /// Check if this is a tool_use event.
    pub fn is_tool_use(&self) -> bool {
        self.r#type == "tool_use"
    }

    /// Get text content if this is a text event.
    pub fn text(&self) -> Option<&str> {
        if self.is_text() {
            self.data.get("text").and_then(|v| v.as_str())
        } else {
            None
        }
    }
}

/// CLI runner for `opencode run`.
pub struct CliRunner {
    rx: mpsc::UnboundedReceiver<CliEvent>,
    _task: tokio::task::JoinHandle<()>,
}

impl CliRunner {
    /// Start the CLI runner with a prompt.
    ///
    /// # Errors
    ///
    /// Returns an error if the process cannot be spawned.
    pub async fn start(prompt: &str, opts: RunOptions) -> Result<Self> {
        let mut cmd = Command::new(&opts.binary);
        cmd.arg("run");

        if let Some(fmt) = &opts.format {
            cmd.arg("--format").arg(fmt);
        }
        if let Some(url) = &opts.attach {
            cmd.arg("--attach").arg(url);
        }
        if opts.continue_session {
            cmd.arg("--continue");
        }
        if let Some(s) = &opts.session {
            cmd.arg("--session").arg(s);
        }
        for f in &opts.file {
            cmd.arg("--file").arg(f);
        }
        if opts.share {
            cmd.arg("--share");
        }
        if let Some(m) = &opts.model {
            cmd.arg("--model").arg(m);
        }
        if let Some(a) = &opts.agent {
            cmd.arg("--agent").arg(a);
        }
        if let Some(t) = &opts.title {
            cmd.arg("--title").arg(t);
        }
        if let Some(p) = opts.port {
            cmd.arg("--port").arg(p.to_string());
        }
        if let Some(c) = &opts.command {
            cmd.arg("--command").arg(c);
        }

        cmd.arg("--").arg(prompt);
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // Inherit stderr to avoid buffer deadlock if CLI writes >64KB
            .kill_on_drop(true);

        if let Some(dir) = &opts.directory {
            cmd.current_dir(dir);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| OpencodeError::Process(format!("Failed to spawn CLI: {}", e)))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| OpencodeError::Process("no stdout from CLI process".into()))?;

        let (tx, rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<CliEvent>(&line) {
                    Ok(evt) => {
                        if tx.send(evt).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse CLI event: {e}");
                    }
                }
            }
            let _ = child.wait().await;
        });

        Ok(Self { rx, _task: task })
    }

    /// Receive the next event.
    ///
    /// Returns `None` if the stream is closed.
    pub async fn recv(&mut self) -> Option<CliEvent> {
        self.rx.recv().await
    }

    /// Collect all text from text events.
    pub async fn collect_text(&mut self) -> String {
        let mut result = String::new();
        while let Some(event) = self.recv().await {
            if let Some(text) = event.text() {
                result.push_str(text);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_options_defaults() {
        let opts = RunOptions::new();
        assert_eq!(opts.format, Some("json".to_string()));
        assert_eq!(opts.binary, "opencode");
        assert!(!opts.continue_session);
        assert!(!opts.share);
    }

    #[test]
    fn test_run_options_builder() {
        let opts = RunOptions::new()
            .model("anthropic/claude-3-5-sonnet")
            .agent("code")
            .title("Test Session")
            .continue_session(true);

        assert_eq!(opts.model, Some("anthropic/claude-3-5-sonnet".to_string()));
        assert_eq!(opts.agent, Some("code".to_string()));
        assert_eq!(opts.title, Some("Test Session".to_string()));
        assert!(opts.continue_session);
    }

    #[test]
    fn test_cli_event_deserialize() {
        let json = r#"{"type":"text","timestamp":1234567890,"sessionID":"s1","text":"Hello"}"#;
        let event: CliEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.r#type, "text");
        assert!(event.is_text());
        assert_eq!(event.text(), Some("Hello"));
    }

    #[test]
    fn test_cli_event_step_start() {
        let json = r#"{"type":"step_start","sessionID":"s1"}"#;
        let event: CliEvent = serde_json::from_str(json).unwrap();
        assert!(event.is_step_start());
        assert!(!event.is_text());
    }
}
