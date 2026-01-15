use crate::error::{ClaudeError, Result};
use crate::types::{InputFormat, Model, OutputFormat, PermissionMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// MCP Server configuration - supports both stdio (subprocess) and HTTP server types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MCPServer {
    /// Stdio MCP server (subprocess)
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        args: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
    },
    /// HTTP MCP server
    #[serde(rename = "http")]
    Http {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
}

impl MCPServer {
    /// Create a new stdio MCP server
    pub fn stdio(command: impl Into<String>, args: Vec<String>) -> Self {
        MCPServer::Stdio {
            command: command.into(),
            args,
            env: None,
        }
    }

    /// Create a new stdio MCP server with environment variables
    pub fn stdio_with_env(
        command: impl Into<String>,
        args: Vec<String>,
        env: HashMap<String, String>,
    ) -> Self {
        MCPServer::Stdio {
            command: command.into(),
            args,
            env: Some(env),
        }
    }

    /// Create a new HTTP MCP server
    pub fn http(url: impl Into<String>) -> Self {
        MCPServer::Http {
            url: url.into(),
            headers: None,
        }
    }

    /// Create a new HTTP MCP server with headers
    pub fn http_with_headers(url: impl Into<String>, headers: HashMap<String, String>) -> Self {
        MCPServer::Http {
            url: url.into(),
            headers: Some(headers),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, MCPServer>,
}

/// Configuration for a Claude CLI session
#[derive(Debug, Clone, Default)]
pub struct SessionConfig {
    /// The query/prompt to send to Claude
    pub query: String,

    // Session semantics
    /// Resume a specific session (maps to --resume)
    pub resume_session_id: Option<String>,
    /// Use a specific session ID (maps to --session-id)
    pub explicit_session_id: Option<String>,
    /// Continue the last session (maps to --continue)
    pub continue_last_session: bool,
    /// Fork an existing session (maps to --fork-session)
    pub fork_session: bool,

    // Models
    /// Primary model to use
    pub model: Option<Model>,
    /// Fallback model if primary is unavailable (maps to --fallback-model)
    pub fallback_model: Option<Model>,

    // Formats
    /// Output format (maps to --output-format)
    pub output_format: OutputFormat,
    /// Input format (maps to --input-format)
    pub input_format: Option<InputFormat>,

    // MCP
    /// MCP server configuration (maps to --mcp-config)
    pub mcp_config: Option<MCPConfig>,
    /// Strict MCP config validation (maps to --strict-mcp-config)
    pub strict_mcp_config: bool,

    // Permissions
    /// Permission mode (maps to --permission-mode)
    pub permission_mode: Option<PermissionMode>,
    /// Skip permission checks dangerously (maps to --dangerously-skip-permissions)
    pub dangerously_skip_permissions: bool,
    /// Allow dangerous permission skipping (maps to --allow-dangerously-skip-permissions)
    pub allow_dangerously_skip_permissions: bool,

    // Prompts
    /// System prompt override (maps to --system-prompt)
    pub system_prompt: Option<String>,
    /// Append to system prompt (maps to --append-system-prompt)
    pub append_system_prompt: Option<String>,

    // Tools and filtering
    /// Specific tools to enable (maps to --tools)
    pub tools: Option<Vec<String>>,
    /// Tools to allow (maps to --allowedTools)
    pub allowed_tools: Option<Vec<String>>,
    /// Tools to disallow (maps to --disallowedTools)
    pub disallowed_tools: Option<Vec<String>>,

    // Output shaping
    /// JSON schema for structured output (maps to --json-schema)
    pub json_schema: Option<String>,
    /// Include partial messages in stream (maps to --include-partial-messages)
    pub include_partial_messages: bool,
    /// Replay user messages (maps to --replay-user-messages)
    pub replay_user_messages: bool,

    // Configuration
    /// Settings JSON (maps to --settings)
    pub settings: Option<String>,
    /// Setting sources (maps to --setting-sources)
    pub setting_sources: Option<Vec<String>>,

    // Directories and plugins
    /// Additional directories to add to context (maps to --add-dir, repeatable)
    pub additional_dirs: Vec<PathBuf>,
    /// Plugin directories (maps to --plugin-dir, repeatable)
    pub plugin_dirs: Vec<PathBuf>,
    /// IDE mode (maps to --ide)
    pub ide: bool,

    // Advanced
    /// Agents configuration JSON (maps to --agents)
    pub agents: Option<String>,
    /// Enable debug mode (maps to --debug)
    pub debug: bool,
    /// Debug filter pattern
    pub debug_filter: Option<String>,

    // Process control
    /// Working directory for the Claude process
    pub working_dir: Option<PathBuf>,
    /// Environment variables to inject into the Claude process
    pub env: Option<HashMap<String, String>>,

    // Misc
    /// Enable verbose output
    pub verbose: bool,
}

impl SessionConfig {
    pub fn builder(query: impl Into<String>) -> SessionConfigBuilder {
        SessionConfigBuilder::new(query)
    }

    pub fn validate(&self) -> Result<()> {
        if self.query.is_empty() {
            return Err(ClaudeError::InvalidConfiguration {
                message: "Query cannot be empty".to_string(),
            });
        }

        // Mutually exclusive session controls
        if self.continue_last_session && self.resume_session_id.is_some() {
            return Err(ClaudeError::InvalidConfiguration {
                message: "Cannot set both continue_last_session and resume_session_id".to_string(),
            });
        }
        if self.resume_session_id.is_some() && self.explicit_session_id.is_some() {
            return Err(ClaudeError::InvalidConfiguration {
                message: "Cannot set both resume_session_id and explicit_session_id".to_string(),
            });
        }

        // Safe dangerous permissions: both must be set together or neither
        if self.dangerously_skip_permissions ^ self.allow_dangerously_skip_permissions {
            return Err(ClaudeError::InvalidConfiguration {
                message: "Dangerous permissions require both flags enabled together (use enable_dangerous_permissions())".to_string(),
            });
        }

        Ok(())
    }
}

/// Builder for SessionConfig with fluent API
pub struct SessionConfigBuilder {
    config: SessionConfig,
}

impl SessionConfigBuilder {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            config: SessionConfig {
                query: query.into(),
                ..Default::default()
            },
        }
    }

    // Session semantics
    /// Resume a specific session by ID (maps to --resume)
    pub fn resume_session_id(mut self, id: impl Into<String>) -> Self {
        self.config.resume_session_id = Some(id.into());
        self
    }

    /// Use a specific session ID (maps to --session-id)
    pub fn explicit_session_id(mut self, id: impl Into<String>) -> Self {
        self.config.explicit_session_id = Some(id.into());
        self
    }

    /// Continue the last session (maps to --continue)
    pub fn continue_last_session(mut self, yes: bool) -> Self {
        self.config.continue_last_session = yes;
        self
    }

    /// Fork an existing session (maps to --fork-session)
    pub fn fork_session(mut self, yes: bool) -> Self {
        self.config.fork_session = yes;
        self
    }

    // Models
    /// Set the primary model
    pub fn model(mut self, model: Model) -> Self {
        self.config.model = Some(model);
        self
    }

    /// Set the fallback model (maps to --fallback-model)
    pub fn fallback_model(mut self, model: Model) -> Self {
        self.config.fallback_model = Some(model);
        self
    }

    // Formats
    /// Set the output format
    pub fn output_format(mut self, format: OutputFormat) -> Self {
        self.config.output_format = format;
        self
    }

    /// Set the input format (maps to --input-format)
    pub fn input_format(mut self, format: InputFormat) -> Self {
        self.config.input_format = Some(format);
        self
    }

    // MCP
    /// Set MCP server configuration
    pub fn mcp_config(mut self, config: MCPConfig) -> Self {
        self.config.mcp_config = Some(config);
        self
    }

    /// Enable strict MCP config validation (maps to --strict-mcp-config)
    pub fn strict_mcp_config(mut self, yes: bool) -> Self {
        self.config.strict_mcp_config = yes;
        self
    }

    // Permissions
    /// Set permission mode (maps to --permission-mode)
    pub fn permission_mode(mut self, mode: PermissionMode) -> Self {
        self.config.permission_mode = Some(mode);
        self
    }

    /// Enable dangerous permission skipping.
    /// This sets both --allow-dangerously-skip-permissions and --dangerously-skip-permissions.
    /// Use with extreme caution.
    pub fn enable_dangerous_permissions(mut self) -> Self {
        self.config.allow_dangerously_skip_permissions = true;
        self.config.dangerously_skip_permissions = true;
        self
    }

    // Prompts
    /// Set system prompt override (maps to --system-prompt)
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.system_prompt = Some(prompt.into());
        self
    }

    /// Append to system prompt (maps to --append-system-prompt)
    pub fn append_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.append_system_prompt = Some(prompt.into());
        self
    }

    // Tools
    /// Set specific tools to enable (maps to --tools)
    pub fn tools(mut self, tools: Vec<String>) -> Self {
        self.config.tools = Some(tools);
        self
    }

    /// Set allowed tools (maps to --allowedTools)
    pub fn allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.config.allowed_tools = Some(tools);
        self
    }

    /// Set disallowed tools (maps to --disallowedTools)
    pub fn disallowed_tools(mut self, tools: Vec<String>) -> Self {
        self.config.disallowed_tools = Some(tools);
        self
    }

    /// Add a single tool to allowed list
    pub fn allow_tool(mut self, tool: impl Into<String>) -> Self {
        self.config
            .allowed_tools
            .get_or_insert_with(Vec::new)
            .push(tool.into());
        self
    }

    /// Add a single tool to disallowed list
    pub fn disallow_tool(mut self, tool: impl Into<String>) -> Self {
        self.config
            .disallowed_tools
            .get_or_insert_with(Vec::new)
            .push(tool.into());
        self
    }

    // Output shaping
    /// Set JSON schema for structured output (maps to --json-schema)
    pub fn json_schema(mut self, schema: impl Into<String>) -> Self {
        self.config.json_schema = Some(schema.into());
        self
    }

    /// Include partial messages in stream (maps to --include-partial-messages)
    pub fn include_partial_messages(mut self, yes: bool) -> Self {
        self.config.include_partial_messages = yes;
        self
    }

    /// Replay user messages (maps to --replay-user-messages)
    pub fn replay_user_messages(mut self, yes: bool) -> Self {
        self.config.replay_user_messages = yes;
        self
    }

    // Configuration
    /// Set settings JSON (maps to --settings)
    pub fn settings(mut self, s: impl Into<String>) -> Self {
        self.config.settings = Some(s.into());
        self
    }

    /// Set setting sources (maps to --setting-sources)
    pub fn setting_sources(mut self, sources: Vec<String>) -> Self {
        self.config.setting_sources = Some(sources);
        self
    }

    // Directories and plugins
    /// Add a directory to context (maps to --add-dir, repeatable)
    pub fn add_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config.additional_dirs.push(dir.into());
        self
    }

    /// Add a plugin directory (maps to --plugin-dir, repeatable)
    pub fn plugin_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config.plugin_dirs.push(dir.into());
        self
    }

    /// Enable IDE mode (maps to --ide)
    pub fn ide(mut self, yes: bool) -> Self {
        self.config.ide = yes;
        self
    }

    // Advanced
    /// Set agents configuration JSON (maps to --agents)
    pub fn agents(mut self, json: impl Into<String>) -> Self {
        self.config.agents = Some(json.into());
        self
    }

    /// Enable debug mode (maps to --debug)
    pub fn debug(mut self, yes: bool) -> Self {
        self.config.debug = yes;
        self
    }

    /// Set debug filter pattern
    pub fn debug_filter(mut self, filter: impl Into<String>) -> Self {
        self.config.debug_filter = Some(filter.into());
        self
    }

    // Process control
    /// Set working directory for the Claude process
    pub fn working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config.working_dir = Some(dir.into());
        self
    }

    /// Set environment variables to inject into the Claude process
    pub fn env(mut self, env: HashMap<String, String>) -> Self {
        self.config.env = Some(env);
        self
    }

    /// Add a single environment variable
    pub fn env_var(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.config
            .env
            .get_or_insert_with(HashMap::new)
            .insert(key.into(), val.into());
        self
    }

    // Misc
    /// Enable verbose output
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.config.verbose = verbose;
        self
    }

    /// Build the SessionConfig, validating all settings
    pub fn build(self) -> Result<SessionConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_config_validation_empty_query() {
        let config = SessionConfig::builder("").build();
        assert!(config.is_err());
        assert!(
            config
                .unwrap_err()
                .to_string()
                .contains("Query cannot be empty")
        );
    }

    #[test]
    fn test_session_config_validation_valid() {
        let config = SessionConfig::builder("test query").build();
        assert!(config.is_ok());
    }

    #[test]
    fn test_session_config_validation_session_conflicts() {
        // continue + resume should fail
        let config = SessionConfig {
            query: "test".to_string(),
            continue_last_session: true,
            resume_session_id: Some("id".to_string()),
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("continue_last_session and resume_session_id")
        );

        // resume + explicit should fail
        let config = SessionConfig {
            query: "test".to_string(),
            resume_session_id: Some("id1".to_string()),
            explicit_session_id: Some("id2".to_string()),
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("resume_session_id and explicit_session_id")
        );
    }

    #[test]
    fn test_session_config_validation_dangerous_permissions() {
        // Only one dangerous flag set should fail
        let config = SessionConfig {
            query: "test".to_string(),
            dangerously_skip_permissions: true,
            allow_dangerously_skip_permissions: false,
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("enable_dangerous_permissions")
        );

        // Both set should succeed
        let config = SessionConfig {
            query: "test".to_string(),
            dangerously_skip_permissions: true,
            allow_dangerously_skip_permissions: true,
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_enable_dangerous_permissions() {
        let config = SessionConfig::builder("test")
            .enable_dangerous_permissions()
            .build()
            .unwrap();

        assert!(config.dangerously_skip_permissions);
        assert!(config.allow_dangerously_skip_permissions);
    }

    #[test]
    fn test_session_config_builder() {
        let config = SessionConfig::builder("my query")
            .resume_session_id("test-id")
            .model(Model::Sonnet)
            .output_format(OutputFormat::Json)
            .verbose(true)
            .build()
            .unwrap();

        assert_eq!(config.query, "my query");
        assert_eq!(config.resume_session_id.as_deref(), Some("test-id"));
        assert_eq!(config.model, Some(Model::Sonnet));
        assert_eq!(config.output_format, OutputFormat::Json);
        assert!(config.verbose);
    }

    #[test]
    fn test_session_config_builder_new_fields() {
        let config = SessionConfig::builder("query")
            .fallback_model(Model::Haiku)
            .input_format(InputFormat::StreamJson)
            .permission_mode(PermissionMode::AcceptEdits)
            .strict_mcp_config(true)
            .json_schema(r#"{"type":"object"}"#)
            .include_partial_messages(true)
            .replay_user_messages(true)
            .tools(vec!["Read".to_string(), "Write".to_string()])
            .settings(r#"{"key":"value"}"#)
            .setting_sources(vec!["source1".to_string()])
            .add_dir("/tmp/dir1")
            .add_dir("/tmp/dir2")
            .plugin_dir("/tmp/plugins")
            .ide(true)
            .agents(r#"{"agent":"config"}"#)
            .debug(true)
            .debug_filter("filter*")
            .env_var("KEY", "VALUE")
            .build()
            .unwrap();

        assert_eq!(config.fallback_model, Some(Model::Haiku));
        assert_eq!(config.input_format, Some(InputFormat::StreamJson));
        assert_eq!(config.permission_mode, Some(PermissionMode::AcceptEdits));
        assert!(config.strict_mcp_config);
        assert_eq!(config.json_schema.as_deref(), Some(r#"{"type":"object"}"#));
        assert!(config.include_partial_messages);
        assert!(config.replay_user_messages);
        assert_eq!(
            config.tools,
            Some(vec!["Read".to_string(), "Write".to_string()])
        );
        assert_eq!(config.settings.as_deref(), Some(r#"{"key":"value"}"#));
        assert_eq!(config.setting_sources, Some(vec!["source1".to_string()]));
        assert_eq!(config.additional_dirs.len(), 2);
        assert_eq!(config.plugin_dirs.len(), 1);
        assert!(config.ide);
        assert_eq!(config.agents.as_deref(), Some(r#"{"agent":"config"}"#));
        assert!(config.debug);
        assert_eq!(config.debug_filter.as_deref(), Some("filter*"));
        assert_eq!(config.env.as_ref().unwrap().get("KEY").unwrap(), "VALUE");
    }

    #[test]
    fn test_default_output_format() {
        let config = SessionConfig::builder("test").build().unwrap();

        assert_eq!(config.output_format, OutputFormat::StreamingJson);
    }

    #[test]
    fn test_mcp_config_serialization_stdio() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            MCPServer::stdio("cmd", vec!["arg1".to_string(), "arg2".to_string()]),
        );

        let mcp_config = MCPConfig {
            mcp_servers: servers,
        };
        let json = serde_json::to_string(&mcp_config).unwrap();

        // Verify JSON contains type field
        assert!(json.contains(r#""type":"stdio""#));

        let deserialized: MCPConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.mcp_servers.len(), 1);
        assert!(deserialized.mcp_servers.contains_key("test"));

        // Verify it's a stdio server
        match &deserialized.mcp_servers["test"] {
            MCPServer::Stdio { command, args, env } => {
                assert_eq!(command, "cmd");
                assert_eq!(args, &vec!["arg1".to_string(), "arg2".to_string()]);
                assert!(env.is_none());
            }
            _ => panic!("Expected Stdio server"),
        }
    }

    #[test]
    fn test_mcp_config_serialization_http() {
        let mut servers = HashMap::new();
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        servers.insert(
            "http-server".to_string(),
            MCPServer::http_with_headers("https://example.com/mcp", headers),
        );

        let mcp_config = MCPConfig {
            mcp_servers: servers,
        };
        let json = serde_json::to_string(&mcp_config).unwrap();

        // Verify JSON contains type field
        assert!(json.contains(r#""type":"http""#));

        let deserialized: MCPConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.mcp_servers.len(), 1);
        assert!(deserialized.mcp_servers.contains_key("http-server"));

        // Verify it's an http server
        match &deserialized.mcp_servers["http-server"] {
            MCPServer::Http { url, headers } => {
                assert_eq!(url, "https://example.com/mcp");
                assert!(headers.is_some());
                assert_eq!(headers.as_ref().unwrap()["Authorization"], "Bearer token");
            }
            _ => panic!("Expected Http server"),
        }
    }

    #[test]
    fn test_mcp_config_mixed_servers() {
        let mut servers = HashMap::new();
        servers.insert(
            "stdio-server".to_string(),
            MCPServer::stdio("node", vec!["server.js".to_string()]),
        );
        servers.insert(
            "http-server".to_string(),
            MCPServer::http("https://api.example.com/mcp"),
        );

        let mcp_config = MCPConfig {
            mcp_servers: servers,
        };
        let json = serde_json::to_string(&mcp_config).unwrap();

        let deserialized: MCPConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.mcp_servers.len(), 2);

        assert!(matches!(
            &deserialized.mcp_servers["stdio-server"],
            MCPServer::Stdio { .. }
        ));
        assert!(matches!(
            &deserialized.mcp_servers["http-server"],
            MCPServer::Http { .. }
        ));
    }
}
