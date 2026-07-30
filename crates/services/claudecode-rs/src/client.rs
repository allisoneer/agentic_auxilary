use crate::config::MCPConfig;
use crate::config::SessionConfig;
use crate::config::serialize_mcp_config;
use crate::error::ClaudeError;
use crate::error::Result;
use crate::process::ProcessHandle;
use crate::process::expand_tilde;
use crate::process::find_claude_in_path;
use crate::process::is_sensitive_key;
use crate::process::redact_argv;
use crate::session::Session;
use crate::types::InvocationMetadata;
use crate::types::Result as ClaudeResult;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::fs;
use tracing::debug;

const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

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
        let redacted_args = redact_argv(&args);
        debug!("Launching claude with args: {:?}", redacted_args);

        let working_dir = resolve_working_dir(config.working_dir.as_deref()).await?;
        let invocation = InvocationMetadata {
            sdk_version: crate::VERSION.to_string(),
            claude_version: self.claude_version().await,
            argv: redacted_args,
            working_dir: Some(working_dir.clone()),
            setting_sources: config.setting_sources.clone(),
            environment_keys: config
                .env
                .as_ref()
                .map(|env| {
                    let mut keys = env.keys().cloned().collect::<Vec<_>>();
                    keys.sort();
                    keys
                })
                .unwrap_or_default(),
            mcp_config: config.mcp_config.as_ref().and_then(|mcp| {
                serialize_mcp_config(mcp, &config.mcp_server_load_policy)
                    .ok()
                    .map(|mut value| {
                        redact_json(&mut value);
                        value
                    })
            }),
        };

        let process = ProcessHandle::spawn(
            &self.claude_path,
            args,
            Some(&working_dir),
            config.env.as_ref(),
        )
        .await?;

        // Store the temp file in the session to keep it alive
        let mut session = Session::new_with_invocation(config, process, invocation).await?;
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

    async fn claude_version(&self) -> Option<String> {
        let mut command = tokio::process::Command::new(&self.claude_path);
        command.arg("--version").kill_on_drop(true);
        tokio::time::timeout(VERSION_PROBE_TIMEOUT, command.output())
            .await
            .ok()
            .and_then(std::result::Result::ok)
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|value| value.trim().chars().take(256).collect())
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
            let temp_file = self
                .write_mcp_config(mcp, &config.mcp_server_load_policy)
                .await?;
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

    async fn write_mcp_config(
        &self,
        config: &MCPConfig,
        policy: &crate::config::MCPServerLoadPolicy,
    ) -> Result<NamedTempFile> {
        let temp_file = NamedTempFile::new()?;
        let json = serde_json::to_string_pretty(&serialize_mcp_config(config, policy)?)?;
        fs::write(temp_file.path(), json).await?;
        Ok(temp_file)
    }
}

async fn resolve_working_dir(path: Option<&Path>) -> Result<PathBuf> {
    let expanded = match path {
        Some(path) => expand_tilde(path.to_string_lossy().as_ref()),
        None => std::env::current_dir()?,
    };
    let canonical =
        fs::canonicalize(&expanded)
            .await
            .map_err(|error| ClaudeError::InvalidConfiguration {
                message: format!(
                    "Working directory {} is invalid: {error}",
                    expanded.display()
                ),
            })?;
    let metadata = fs::metadata(&canonical).await?;
    if !metadata.is_dir() {
        return Err(ClaudeError::InvalidConfiguration {
            message: format!(
                "Working directory {} is not a directory",
                canonical.display()
            ),
        });
    }
    Ok(canonical)
}

fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if is_sensitive_key(key) {
                    *child = serde_json::Value::String("<redacted>".to_string());
                } else {
                    redact_json(child);
                }
            }
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(redact_json),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::InputFormat;
    use crate::types::OutputFormat;
    use crate::types::PermissionMode;

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
    async fn empty_tools_and_setting_sources_emit_explicit_empty_arguments() {
        let client = create_test_client();
        let config = SessionConfig::builder("test")
            .tools(vec![])
            .setting_sources(vec![])
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();
        let tools_pos = args.iter().position(|arg| arg == "--tools").unwrap();
        let sources_pos = args
            .iter()
            .position(|arg| arg == "--setting-sources")
            .unwrap();

        assert_eq!(args.get(tools_pos + 1).map(String::as_str), Some(""));
        assert_eq!(args.get(sources_pos + 1).map(String::as_str), Some(""));
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
            .input_format(InputFormat::Text)
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();

        let input_pos = args.iter().position(|a| a == "--input-format").unwrap();
        assert_eq!(args[input_pos + 1], "text");
    }

    #[tokio::test]
    async fn test_build_args_with_output_shaping() {
        let client = create_test_client();
        let config = SessionConfig::builder("test")
            .json_schema(r#"{"type":"object"}"#)
            .include_partial_messages(true)
            .replay_user_messages(false)
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let (args, _) = client.build_args(&config).await.unwrap();

        let schema_pos = args.iter().position(|a| a == "--json-schema").unwrap();
        assert_eq!(args[schema_pos + 1], r#"{"type":"object"}"#);
        assert!(args.contains(&"--include-partial-messages".to_string()));
        assert!(!args.contains(&"--replay-user-messages".to_string()));
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

    #[test]
    fn invocation_redaction_hides_prompts_settings_and_query() {
        let args = vec![
            "--system-prompt".to_string(),
            "secret prompt".to_string(),
            "--settings".to_string(),
            "{\"token\":\"secret\"}".to_string(),
            "--agents".to_string(),
            "{\"reviewer\":\"agent secret\"}".to_string(),
            "--".to_string(),
            "secret query".to_string(),
        ];
        let redacted = redact_argv(&args);
        assert_eq!(redacted[1], "<redacted>");
        assert_eq!(redacted[3], "<redacted>");
        assert_eq!(redacted[5], "<redacted>");
        assert_eq!(redacted[7], "<redacted>");
        assert!(!redacted.join(" ").contains("secret"));
    }

    #[test]
    fn invocation_redaction_hides_inline_agents_json() {
        let redacted = redact_argv(&["--agents={\"token\":\"agent secret\"}".to_string()]);
        assert_eq!(redacted, ["--agents=<redacted>"]);
        assert!(!redacted.join(" ").contains("agent secret"));
    }

    #[test]
    fn recursive_json_redaction_hides_credential_values() {
        let mut value = serde_json::json!({
            "headers": {"Authorization": "Bearer secret"},
            "env": {"service_credential": "secret", "SAFE": "visible"}
        });
        redact_json(&mut value);
        let rendered = value.to_string();
        assert!(!rendered.contains("Bearer secret"));
        assert!(!rendered.contains("\"secret\""));
        assert!(rendered.contains("visible"));
    }

    #[tokio::test]
    async fn working_directory_is_canonicalized_and_must_be_a_directory() {
        let directory = tempfile::TempDir::new().unwrap();
        let canonical = fs::canonicalize(directory.path()).await.unwrap();
        assert_eq!(
            resolve_working_dir(Some(directory.path())).await.unwrap(),
            canonical
        );

        assert_eq!(
            resolve_working_dir(None).await.unwrap(),
            fs::canonicalize(std::env::current_dir().unwrap())
                .await
                .unwrap()
        );

        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(resolve_working_dir(Some(file.path())).await.is_err());
        assert!(
            resolve_working_dir(Some(Path::new("/definitely/missing/claude-cwd")))
                .await
                .is_err()
        );
    }
}
