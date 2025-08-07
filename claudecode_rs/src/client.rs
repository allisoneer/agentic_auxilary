use crate::config::{MCPConfig, SessionConfig};
use crate::error::{ClaudeError, Result};
use crate::process::{expand_tilde, find_claude_in_path, ProcessHandle};
use crate::session::Session;
use crate::types::Result as ClaudeResult;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use tokio::fs;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct Client {
    claude_path: PathBuf,
}

impl Client {
    /// Create a new client by finding claude in PATH
    pub async fn new() -> Result<Self> {
        let claude_path = find_claude_in_path().await?;
        Ok(Self { claude_path })
    }

    /// Create a new client with a specific claude path
    pub async fn with_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !fs::try_exists(path).await.unwrap_or(false) {
            return Err(ClaudeError::ClaudeNotFoundAtPath {
                path: path.to_path_buf(),
            });
        }
        Ok(Self {
            claude_path: path.to_path_buf(),
        })
    }

    /// Launch a new Claude session asynchronously
    pub async fn launch(&self, config: SessionConfig) -> Result<Session> {
        config.validate()?;

        let (args, mcp_file) = self.build_args(&config).await?;
        debug!("Launching claude with args: {:?}", args);

        // Prepare working directory
        let working_dir = config
            .working_dir
            .as_ref()
            .map(|dir| expand_tilde(dir.to_str().unwrap_or("")));

        let process = ProcessHandle::spawn(&self.claude_path, args, working_dir.as_deref()).await?;

        // Store the temp file in the session to keep it alive
        let mut session = Session::new(config, process).await?;
        if let Some(temp_file) = mcp_file {
            session.set_mcp_temp_file(temp_file);
        }
        Ok(session)
    }

    /// Launch a session and wait for it to complete
    pub async fn launch_and_wait(&self, config: SessionConfig) -> Result<ClaudeResult> {
        let session = self.launch(config).await?;
        session.wait().await
    }

    async fn build_args(
        &self,
        config: &SessionConfig,
    ) -> Result<(Vec<String>, Option<NamedTempFile>)> {
        let mut args = Vec::new();
        let mut mcp_file = None;

        // Add print flag for non-interactive mode
        args.push("--print".to_string());

        // Add query
        args.push(config.query.clone());

        // Add optional arguments
        if let Some(ref session_id) = config.session_id {
            args.push("--resume".to_string());
            args.push(session_id.clone());
        }

        if let Some(model) = config.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }

        args.push("--output-format".to_string());
        args.push(config.output_format.to_string());

        // Handle MCP config
        if let Some(ref mcp) = config.mcp_config {
            let temp_file = self.write_mcp_config(mcp).await?;
            args.push("--mcp-config".to_string());
            args.push(temp_file.path().to_string_lossy().to_string());
            mcp_file = Some(temp_file);
        }

        if let Some(ref tool) = config.permission_prompt_tool {
            args.push("--permission-prompt-tool".to_string());
            args.push(tool.clone());
        }

        if let Some(turns) = config.max_turns {
            args.push("--max-turns".to_string());
            args.push(turns.to_string());
        }

        if let Some(ref prompt) = config.system_prompt {
            args.push("--system-prompt".to_string());
            args.push(prompt.clone());
        }

        if let Some(ref prompt) = config.append_system_prompt {
            args.push("--append-system-prompt".to_string());
            args.push(prompt.clone());
        }

        if let Some(ref tools) = config.allowed_tools {
            args.push("--allowedTools".to_string());
            args.push(tools.join(","));
        }

        if let Some(ref tools) = config.disallowed_tools {
            args.push("--disallowedTools".to_string());
            args.push(tools.join(","));
        }

        // Auto-add verbose flag for streaming JSON or if explicitly requested
        if config.output_format == crate::types::OutputFormat::StreamingJson || config.verbose {
            args.push("--verbose".to_string());
        }

        if let Some(ref instructions) = config.custom_instructions {
            args.push("--custom-instructions".to_string());
            args.push(instructions.clone());
        }

        Ok((args, mcp_file))
    }

    async fn write_mcp_config(&self, config: &MCPConfig) -> Result<NamedTempFile> {
        let temp_file = NamedTempFile::new()?;
        let json = serde_json::to_string_pretty(config)?;
        fs::write(temp_file.path(), json).await?;
        Ok(temp_file)
    }
}
