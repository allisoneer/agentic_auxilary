use crate::error::{ClaudeError, Result};
use crate::types::{Model, OutputFormat};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServer {
    pub command: String,
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, MCPServer>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionConfig {
    pub query: String,
    pub session_id: Option<String>,
    pub model: Option<Model>,
    pub output_format: OutputFormat,
    pub mcp_config: Option<MCPConfig>,
    pub permission_prompt_tool: Option<String>,
    pub working_dir: Option<PathBuf>,
    pub max_turns: Option<u32>,
    pub system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub disallowed_tools: Option<Vec<String>>,
    pub custom_instructions: Option<String>,
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

        if let Some(turns) = self.max_turns
            && turns == 0
        {
            return Err(ClaudeError::InvalidConfiguration {
                message: "Max turns must be greater than 0".to_string(),
            });
        }

        Ok(())
    }
}

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

    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.config.session_id = Some(id.into());
        self
    }

    pub fn model(mut self, model: Model) -> Self {
        self.config.model = Some(model);
        self
    }

    pub fn output_format(mut self, format: OutputFormat) -> Self {
        self.config.output_format = format;
        self
    }

    pub fn mcp_config(mut self, config: MCPConfig) -> Self {
        self.config.mcp_config = Some(config);
        self
    }

    pub fn permission_prompt_tool(mut self, tool: impl Into<String>) -> Self {
        self.config.permission_prompt_tool = Some(tool.into());
        self
    }

    pub fn working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.config.working_dir = Some(dir.into());
        self
    }

    pub fn max_turns(mut self, turns: u32) -> Self {
        self.config.max_turns = Some(turns);
        self
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.system_prompt = Some(prompt.into());
        self
    }

    pub fn append_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.append_system_prompt = Some(prompt.into());
        self
    }

    pub fn allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.config.allowed_tools = Some(tools);
        self
    }

    pub fn disallowed_tools(mut self, tools: Vec<String>) -> Self {
        self.config.disallowed_tools = Some(tools);
        self
    }

    pub fn verbose(mut self, verbose: bool) -> Self {
        self.config.verbose = verbose;
        self
    }

    pub fn allow_tool(mut self, tool: impl Into<String>) -> Self {
        self.config
            .allowed_tools
            .get_or_insert_with(Vec::new)
            .push(tool.into());
        self
    }

    pub fn disallow_tool(mut self, tool: impl Into<String>) -> Self {
        self.config
            .disallowed_tools
            .get_or_insert_with(Vec::new)
            .push(tool.into());
        self
    }

    pub fn custom_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.config.custom_instructions = Some(instructions.into());
        self
    }

    pub fn build(self) -> Result<SessionConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_config_validation() {
        // Empty query should fail
        let config = SessionConfig::builder("").build();
        assert!(config.is_err());

        // Zero max turns should fail
        let config = SessionConfig::builder("test").max_turns(0).build();
        assert!(config.is_err());

        // Valid config should succeed
        let config = SessionConfig::builder("test query").build();
        assert!(config.is_ok());
    }

    #[test]
    fn test_session_config_builder() {
        let config = SessionConfig::builder("my query")
            .session_id("test-id")
            .model(Model::Sonnet)
            .output_format(OutputFormat::Json)
            .max_turns(5)
            .verbose(true)
            .custom_instructions("Always be helpful")
            .build()
            .unwrap();

        assert_eq!(config.query, "my query");
        assert_eq!(config.session_id.as_deref(), Some("test-id"));
        assert_eq!(config.model, Some(Model::Sonnet));
        assert_eq!(config.output_format, OutputFormat::Json);
        assert_eq!(config.max_turns, Some(5));
        assert!(config.verbose);
        assert_eq!(
            config.custom_instructions.as_deref(),
            Some("Always be helpful")
        );
    }

    #[test]
    fn test_default_output_format() {
        let config = SessionConfig::builder("test").build().unwrap();

        assert_eq!(config.output_format, OutputFormat::StreamingJson);
    }

    #[test]
    fn test_mcp_config_serialization() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            MCPServer {
                command: "cmd".to_string(),
                args: vec!["arg1".to_string(), "arg2".to_string()],
                env: None,
            },
        );

        let mcp_config = MCPConfig {
            mcp_servers: servers,
        };
        let json = serde_json::to_string(&mcp_config).unwrap();

        let deserialized: MCPConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.mcp_servers.len(), 1);
        assert!(deserialized.mcp_servers.contains_key("test"));
    }
}
