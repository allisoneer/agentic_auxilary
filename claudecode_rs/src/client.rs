use crate::config::{MCPConfig, SessionConfig};
use crate::error::{ClaudeError, Result};
use crate::process::{ProcessHandle, expand_tilde, find_claude_in_path};
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

        let process = ProcessHandle::spawn(
            &self.claude_path,
            args,
            working_dir.as_deref(),
            config.env.as_ref(),
        )
        .await?;

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

    /// Probe the CLI for supported capabilities.
    ///
    /// This runs `claude --help` and parses the output to detect supported flags.
    /// Useful for validating SDK compatibility with the installed CLI version.
    ///
    /// # Example
    /// ```ignore
    /// let client = Client::new().await?;
    /// let caps = client.probe_cli().await?;
    ///
    /// if caps.supports("--permission-mode") {
    ///     println!("Permission mode is supported");
    /// }
    /// ```
    pub async fn probe_cli(&self) -> Result<crate::probe::CliCapabilities> {
        crate::probe::probe_cli(&self.claude_path).await
    }

    async fn build_args(
        &self,
        config: &SessionConfig,
    ) -> Result<(Vec<String>, Option<NamedTempFile>)> {
        let mut args = Vec::new();
        let mut mcp_file = None;

        // Add print flag for non-interactive mode
        args.push("--print".to_string());

        // Models
        if let Some(model) = config.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        if let Some(model) = config.fallback_model {
            args.push("--fallback-model".to_string());
            args.push(model.to_string());
        }

        // Formats
        args.push("--output-format".to_string());
        args.push(config.output_format.to_string());
        if let Some(ref format) = config.input_format {
            args.push("--input-format".to_string());
            args.push(format.to_string());
        }

        // MCP config
        if let Some(ref mcp) = config.mcp_config {
            let temp_file = self.write_mcp_config(mcp).await?;
            args.push("--mcp-config".to_string());
            args.push(temp_file.path().to_string_lossy().to_string());
            mcp_file = Some(temp_file);
        }
        if config.strict_mcp_config {
            args.push("--strict-mcp-config".to_string());
        }

        // Permissions
        if let Some(ref mode) = config.permission_mode {
            args.push("--permission-mode".to_string());
            args.push(mode.to_string());
        }
        if config.allow_dangerously_skip_permissions {
            args.push("--allow-dangerously-skip-permissions".to_string());
        }
        if config.dangerously_skip_permissions {
            args.push("--dangerously-skip-permissions".to_string());
        }

        // Prompts
        if let Some(ref prompt) = config.system_prompt {
            args.push("--system-prompt".to_string());
            args.push(prompt.clone());
        }
        if let Some(ref prompt) = config.append_system_prompt {
            args.push("--append-system-prompt".to_string());
            args.push(prompt.clone());
        }

        // Tools
        if let Some(ref tools) = config.tools {
            args.push("--tools".to_string());
            args.push(tools.join(","));
        }
        if let Some(ref tools) = config.allowed_tools {
            args.push("--allowedTools".to_string());
            args.push(tools.join(","));
        }
        if let Some(ref tools) = config.disallowed_tools {
            args.push("--disallowedTools".to_string());
            args.push(tools.join(","));
        }

        // Output shaping
        if let Some(ref schema) = config.json_schema {
            args.push("--json-schema".to_string());
            args.push(schema.clone());
        }
        if config.include_partial_messages {
            args.push("--include-partial-messages".to_string());
        }
        if config.replay_user_messages {
            args.push("--replay-user-messages".to_string());
        }

        // Configuration
        if let Some(ref settings) = config.settings {
            args.push("--settings".to_string());
            args.push(settings.clone());
        }
        if let Some(ref sources) = config.setting_sources {
            args.push("--setting-sources".to_string());
            args.push(sources.join(","));
        }

        // Directories and plugins (repeatable flags)
        for dir in &config.additional_dirs {
            let expanded = expand_tilde(dir.to_string_lossy().as_ref());
            let path = tokio::fs::canonicalize(&expanded).await.unwrap_or(expanded);
            args.push("--add-dir".to_string());
            args.push(path.to_string_lossy().to_string());
        }
        for dir in &config.plugin_dirs {
            let expanded = expand_tilde(dir.to_string_lossy().as_ref());
            let path = tokio::fs::canonicalize(&expanded).await.unwrap_or(expanded);
            args.push("--plugin-dir".to_string());
            args.push(path.to_string_lossy().to_string());
        }
        if config.ide {
            args.push("--ide".to_string());
        }

        // Advanced
        if let Some(ref agents) = config.agents {
            args.push("--agents".to_string());
            args.push(agents.clone());
        }
        if config.debug {
            args.push("--debug".to_string());
            if let Some(ref filter) = config.debug_filter {
                args.push(filter.clone());
            }
        }

        // Session semantics
        if let Some(ref id) = config.resume_session_id {
            args.push("--resume".to_string());
            args.push(id.clone());
        }
        if let Some(ref id) = config.explicit_session_id {
            args.push("--session-id".to_string());
            args.push(id.clone());
        }
        if config.continue_last_session {
            args.push("--continue".to_string());
        }
        if config.fork_session {
            args.push("--fork-session".to_string());
        }

        // Auto-add verbose flag for streaming JSON or if explicitly requested
        if config.output_format == crate::types::OutputFormat::StreamingJson || config.verbose {
            args.push("--verbose".to_string());
        }

        // Always add -- separator before query to protect dash-prefixed queries
        args.push("--".to_string());
        args.push(config.query.clone());

        Ok((args, mcp_file))
    }

    async fn write_mcp_config(&self, config: &MCPConfig) -> Result<NamedTempFile> {
        let temp_file = NamedTempFile::new()?;
        let json = serde_json::to_string_pretty(config)?;
        fs::write(temp_file.path(), json).await?;
        Ok(temp_file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{InputFormat, OutputFormat, PermissionMode};

    fn create_test_client() -> Client {
        Client {
            claude_path: PathBuf::from("/usr/bin/claude"),
        }
    }

    #[tokio::test]
    async fn test_build_args_inserts_separator_for_dash_query() {
        let client = create_test_client();
        let config = SessionConfig::builder("- list files")
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();

        // Find the -- separator
        let sep_pos = args.iter().position(|a| a == "--");
        assert!(sep_pos.is_some(), "Separator -- should be present");

        let sep_pos = sep_pos.unwrap();
        assert_eq!(args[sep_pos + 1], "- list files");
    }

    #[tokio::test]
    async fn test_build_args_basic() {
        let client = create_test_client();
        let config = SessionConfig::builder("test query")
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();

        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"text".to_string()));
        assert!(args.contains(&"--".to_string()));
        assert!(args.contains(&"test query".to_string()));
    }

    #[tokio::test]
    async fn test_build_args_with_model() {
        let client = create_test_client();
        let config = SessionConfig::builder("test")
            .model(crate::types::Model::Sonnet)
            .fallback_model(crate::types::Model::Haiku)
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();

        let model_pos = args.iter().position(|a| a == "--model").unwrap();
        assert_eq!(args[model_pos + 1], "sonnet");

        let fallback_pos = args.iter().position(|a| a == "--fallback-model").unwrap();
        assert_eq!(args[fallback_pos + 1], "haiku");
    }

    #[tokio::test]
    async fn test_build_args_with_permissions() {
        let client = create_test_client();
        let config = SessionConfig::builder("test")
            .permission_mode(PermissionMode::AcceptEdits)
            .enable_dangerous_permissions()
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();

        assert!(args.contains(&"--permission-mode".to_string()));
        assert!(args.contains(&"acceptEdits".to_string()));
        assert!(args.contains(&"--allow-dangerously-skip-permissions".to_string()));
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[tokio::test]
    async fn test_build_args_with_tools() {
        let client = create_test_client();
        let config = SessionConfig::builder("test")
            .tools(vec!["Read".to_string(), "Write".to_string()])
            .allowed_tools(vec!["Bash".to_string()])
            .disallowed_tools(vec!["WebSearch".to_string()])
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();

        let tools_pos = args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(args[tools_pos + 1], "Read,Write");

        let allowed_pos = args.iter().position(|a| a == "--allowedTools").unwrap();
        assert_eq!(args[allowed_pos + 1], "Bash");

        let disallowed_pos = args.iter().position(|a| a == "--disallowedTools").unwrap();
        assert_eq!(args[disallowed_pos + 1], "WebSearch");
    }

    #[tokio::test]
    async fn test_build_args_with_session_semantics() {
        let client = create_test_client();

        // Test resume
        let config = SessionConfig::builder("test")
            .resume_session_id("session-123")
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();
        let (args, _) = client.build_args(&config).await.unwrap();
        let resume_pos = args.iter().position(|a| a == "--resume").unwrap();
        assert_eq!(args[resume_pos + 1], "session-123");

        // Test explicit session ID
        let config = SessionConfig::builder("test")
            .explicit_session_id("uuid-456")
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();
        let (args, _) = client.build_args(&config).await.unwrap();
        let session_id_pos = args.iter().position(|a| a == "--session-id").unwrap();
        assert_eq!(args[session_id_pos + 1], "uuid-456");

        // Test continue
        let config = SessionConfig::builder("test")
            .continue_last_session(true)
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();
        let (args, _) = client.build_args(&config).await.unwrap();
        assert!(args.contains(&"--continue".to_string()));

        // Test fork
        let config = SessionConfig::builder("test")
            .fork_session(true)
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();
        let (args, _) = client.build_args(&config).await.unwrap();
        assert!(args.contains(&"--fork-session".to_string()));
    }

    #[tokio::test]
    async fn test_build_args_with_input_format() {
        let client = create_test_client();
        let config = SessionConfig::builder("test")
            .input_format(InputFormat::StreamJson)
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();

        let input_pos = args.iter().position(|a| a == "--input-format").unwrap();
        assert_eq!(args[input_pos + 1], "stream-json");
    }

    #[tokio::test]
    async fn test_build_args_with_output_shaping() {
        let client = create_test_client();
        let config = SessionConfig::builder("test")
            .json_schema(r#"{"type":"object"}"#)
            .include_partial_messages(true)
            .replay_user_messages(true)
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();

        let schema_pos = args.iter().position(|a| a == "--json-schema").unwrap();
        assert_eq!(args[schema_pos + 1], r#"{"type":"object"}"#);
        assert!(args.contains(&"--include-partial-messages".to_string()));
        assert!(args.contains(&"--replay-user-messages".to_string()));
    }

    #[tokio::test]
    async fn test_build_args_with_advanced_options() {
        let client = create_test_client();
        let config = SessionConfig::builder("test")
            .agents(r#"{"test":"config"}"#)
            .debug(true)
            .debug_filter("filter*")
            .ide(true)
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();

        let agents_pos = args.iter().position(|a| a == "--agents").unwrap();
        assert_eq!(args[agents_pos + 1], r#"{"test":"config"}"#);

        let debug_pos = args.iter().position(|a| a == "--debug").unwrap();
        assert_eq!(args[debug_pos + 1], "filter*");

        assert!(args.contains(&"--ide".to_string()));
    }

    #[tokio::test]
    async fn test_build_args_verbose_auto_added_for_streaming() {
        let client = create_test_client();

        // Streaming JSON should auto-add verbose
        let config = SessionConfig::builder("test")
            .output_format(OutputFormat::StreamingJson)
            .build()
            .unwrap();
        let (args, _) = client.build_args(&config).await.unwrap();
        assert!(args.contains(&"--verbose".to_string()));

        // Text format should not auto-add verbose
        let config = SessionConfig::builder("test")
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();
        let (args, _) = client.build_args(&config).await.unwrap();
        assert!(!args.contains(&"--verbose".to_string()));

        // Explicit verbose flag should be added
        let config = SessionConfig::builder("test")
            .output_format(OutputFormat::Text)
            .verbose(true)
            .build()
            .unwrap();
        let (args, _) = client.build_args(&config).await.unwrap();
        assert!(args.contains(&"--verbose".to_string()));
    }
}
