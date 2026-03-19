//! Configuration types for the agentic tools ecosystem.
//!
//! The root type is [`AgenticConfig`], which contains namespaced sub-configs
//! for different concerns: subagents, reasoning, services, orchestrator,
//! web retrieval, CLI tools, and logging.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/// Root configuration for all agentic tools.
///
/// This is the unified configuration that gets loaded from `agentic.toml` files.
/// All fields use `#[serde(default)]` so partial configs work correctly.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AgenticConfig {
    /// Optional JSON Schema URL for IDE autocomplete support.
    /// In TOML: `"$schema" = "file://./agentic.schema.json"`
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Tool-specific config for coding-agent-tools subagents.
    pub subagents: SubagentsConfig,

    /// Tool-specific config for gpt5-reasoner.
    pub reasoning: ReasoningConfig,

    /// External service configurations (Anthropic, Exa).
    pub services: ServicesConfig,

    /// Orchestrator session and timing configuration.
    pub orchestrator: OrchestratorConfig,

    /// Web retrieval tool configuration.
    pub web_retrieval: WebRetrievalConfig,

    /// CLI tools (grep, glob, ls) configuration.
    pub cli_tools: CliToolsConfig,

    /// Logging and diagnostics configuration.
    pub logging: LoggingConfig,
}

//
// ─────────────────────────────────────────────────────────────────────────────
// SUBAGENTS CONFIG
// ─────────────────────────────────────────────────────────────────────────────
//

/// Configuration for coding-agent-tools subagents (`ask_agent` tool).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SubagentsConfig {
    // TODO(3): Model name handling could be more type-safe:
    // - Consider documenting supported models in code (enum or const list)
    // - Standardize approach between anthropic-async, claudecode_rs, and consumers
    // - Current string-based approach works but lacks IDE completion and validation
    /// Model for Locator subagent (fast discovery). Uses Claude CLI format.
    pub locator_model: String,
    /// Model for Analyzer subagent (deep analysis). Uses Claude CLI format.
    pub analyzer_model: String,
}

impl Default for SubagentsConfig {
    fn default() -> Self {
        Self {
            locator_model: "claude-haiku-4-5".into(),
            analyzer_model: "claude-sonnet-4-6".into(),
        }
    }
}

//
// ─────────────────────────────────────────────────────────────────────────────
// REASONING CONFIG
// ─────────────────────────────────────────────────────────────────────────────
//

/// Schema-only enum for `reasoning_effort` IDE autocomplete.
/// Runtime storage remains Option<String> for advisory validation semantics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum ReasoningEffortLevel {
    Low,
    Medium,
    High,
    Xhigh,
}

// Note on external type dependencies: We investigated using model types from the
// async-openai crate but found they use plain `String` for most model fields (chat
// completions, embeddings, assistants, fine-tuning, audio transcription). Only image
// generation (ImageModel) and TTS (SpeechModel) have typed enums, and those include
// `Other(String)` escape hatches with #[serde(untagged)]. Their Model struct (for
// listing available models) also uses `id: String`. Copying their types would not
// improve our type safety since they face the same constraints we do and chose the
// same approach. See research/pr127-group7-type-safety-external-type-dependencies.md.

/// Configuration for gpt5-reasoner tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ReasoningConfig {
    /// `OpenRouter` model ID for optimizer step.
    pub optimizer_model: String,
    /// `OpenRouter` model ID for executor/reasoner step.
    pub executor_model: String,
    /// Optional reasoning effort level: low, medium, high, xhigh.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ReasoningEffortLevel>")]
    pub reasoning_effort: Option<String>,
    /// Optional API base URL override for reasoning service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
    /// Optional token limit for reasoning requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_limit: Option<u32>,
}

impl Default for ReasoningConfig {
    fn default() -> Self {
        Self {
            optimizer_model: "anthropic/claude-sonnet-4.6".into(),
            executor_model: "openai/gpt-5.2".into(),
            reasoning_effort: None,
            api_base_url: None,
            token_limit: None,
        }
    }
}

//
// ─────────────────────────────────────────────────────────────────────────────
// ORCHESTRATOR CONFIG
// ─────────────────────────────────────────────────────────────────────────────
//

/// Configuration for opencode-orchestrator-mcp.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct OrchestratorConfig {
    /// Maximum session duration in seconds (default: 3600 = 1 hour).
    pub session_deadline_secs: u64,
    /// Inactivity timeout in seconds before session ends (default: 300 = 5 minutes).
    pub inactivity_timeout_secs: u64,
    /// Context compaction threshold as fraction 0.0-1.0 (default: 0.80).
    pub compaction_threshold: f64,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            session_deadline_secs: 3600,
            inactivity_timeout_secs: 300,
            compaction_threshold: 0.80,
        }
    }
}

//
// ─────────────────────────────────────────────────────────────────────────────
// WEB RETRIEVAL CONFIG
// ─────────────────────────────────────────────────────────────────────────────
//

/// Configuration for web-retrieval tools (`web_fetch`, `web_search`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct WebRetrievalConfig {
    /// HTTP request timeout in seconds (default: 30).
    pub request_timeout_secs: u64,
    /// Default maximum bytes to fetch (default: 5MB).
    pub default_max_bytes: u64,
    /// Default number of search results (default: 8).
    pub default_search_results: u32,
    /// Maximum number of search results allowed (default: 20).
    pub max_search_results: u32,
    /// Summarizer configuration for Haiku-based summarization.
    pub summarizer: WebSummarizerConfig,
}

impl Default for WebRetrievalConfig {
    fn default() -> Self {
        Self {
            request_timeout_secs: 30,
            default_max_bytes: 5 * 1024 * 1024, // 5MB
            default_search_results: 8,
            max_search_results: 20,
            summarizer: WebSummarizerConfig::default(),
        }
    }
}

/// Configuration for the web summarizer (Haiku).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct WebSummarizerConfig {
    /// Model to use for summarization (default: claude-haiku-4-5).
    pub model: String,
    /// Maximum tokens for summary output (default: 300).
    pub max_tokens: u32,
    /// Temperature for summary generation (default: 0.2).
    pub temperature: f64,
}

impl Default for WebSummarizerConfig {
    fn default() -> Self {
        Self {
            model: "claude-haiku-4-5".into(),
            max_tokens: 300,
            temperature: 0.2,
        }
    }
}

//
// ─────────────────────────────────────────────────────────────────────────────
// CLI TOOLS CONFIG
// ─────────────────────────────────────────────────────────────────────────────
//

/// Configuration for CLI tools (grep, glob, ls).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CliToolsConfig {
    /// Default page size for ls results (default: 100).
    pub ls_page_size: u32,
    /// Default `head_limit` for grep results (default: 200).
    pub grep_default_limit: u32,
    /// Default `head_limit` for glob results (default: 500).
    pub glob_default_limit: u32,
    /// Maximum directory traversal depth (default: 10).
    pub max_depth: u32,
    /// Pagination cache TTL in seconds (default: 300 = 5 minutes).
    pub pagination_cache_ttl_secs: u64,
    /// Additional ignore patterns to append to builtin ignores.
    #[serde(default)]
    pub extra_ignore_patterns: Vec<String>,
}

impl Default for CliToolsConfig {
    fn default() -> Self {
        Self {
            ls_page_size: 100,
            grep_default_limit: 200,
            glob_default_limit: 500,
            max_depth: 10,
            pagination_cache_ttl_secs: 300,
            extra_ignore_patterns: vec![],
        }
    }
}

//
// ─────────────────────────────────────────────────────────────────────────────
// SERVICES CONFIG
// ─────────────────────────────────────────────────────────────────────────────
//

/// External service configurations.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ServicesConfig {
    /// Anthropic API configuration.
    pub anthropic: AnthropicServiceConfig,
    /// Exa search API configuration.
    pub exa: ExaServiceConfig,
}

/// Anthropic API service configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AnthropicServiceConfig {
    /// Base URL for the Anthropic API.
    pub base_url: String,
}

impl Default for AnthropicServiceConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.anthropic.com".into(),
        }
    }
}

/// Exa search API service configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ExaServiceConfig {
    /// Base URL for the Exa API.
    pub base_url: String,
}

impl Default for ExaServiceConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.exa.ai".into(),
        }
    }
}

//
// ─────────────────────────────────────────────────────────────────────────────
// LOGGING CONFIG
// ─────────────────────────────────────────────────────────────────────────────
//

/// Logging and diagnostics configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error).
    pub level: String,

    /// Whether to enable JSON-formatted logs.
    pub json: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            json: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_serializes() {
        let config = AgenticConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("[subagents]"));
        assert!(toml_str.contains("[reasoning]"));
        // Services sections serialize as [services.anthropic], [services.exa], etc.
        assert!(toml_str.contains("[services.anthropic]"));
        assert!(toml_str.contains("[services.exa]"));
        assert!(toml_str.contains("[orchestrator]"));
        assert!(toml_str.contains("[web_retrieval]"));
        assert!(toml_str.contains("[cli_tools]"));
        assert!(toml_str.contains("[logging]"));
        // Ensure old sections are NOT present
        assert!(!toml_str.contains("[thoughts]"));
        assert!(!toml_str.contains("[models]"));
    }

    #[test]
    fn test_default_models_use_undated_names() {
        let subagents = SubagentsConfig::default();
        assert!(!subagents.locator_model.contains("20"));
        assert!(!subagents.analyzer_model.contains("20"));

        let reasoning = ReasoningConfig::default();
        assert!(!reasoning.optimizer_model.contains("20"));
        assert!(!reasoning.executor_model.contains("20"));
    }

    #[test]
    fn test_partial_config_deserializes() {
        let toml_str = r#"
[subagents]
locator_model = "custom-model"
"#;
        let config: AgenticConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.subagents.locator_model, "custom-model");
        // Other fields get defaults
        assert_eq!(config.subagents.analyzer_model, "claude-sonnet-4-6");
        assert_eq!(
            config.services.anthropic.base_url,
            "https://api.anthropic.com"
        );
    }

    #[test]
    fn test_schema_field_optional() {
        let toml_str = r#""$schema" = "file://./agentic.schema.json""#;
        let config: AgenticConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.schema, Some("file://./agentic.schema.json".into()));
    }

    // Default value assertion tests - ensure defaults match current hardcoded behavior
    #[test]
    fn test_web_retrieval_defaults_match_hardcoded() {
        let cfg = WebRetrievalConfig::default();
        assert_eq!(cfg.request_timeout_secs, 30);
        assert_eq!(cfg.default_max_bytes, 5 * 1024 * 1024); // 5MB
        assert_eq!(cfg.default_search_results, 8);
        assert_eq!(cfg.max_search_results, 20);
        assert_eq!(cfg.summarizer.model, "claude-haiku-4-5");
        assert_eq!(cfg.summarizer.max_tokens, 300);
        assert!((cfg.summarizer.temperature - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cli_tools_defaults_match_hardcoded() {
        let cfg = CliToolsConfig::default();
        assert_eq!(cfg.ls_page_size, 100);
        assert_eq!(cfg.grep_default_limit, 200);
        assert_eq!(cfg.glob_default_limit, 500);
        assert_eq!(cfg.max_depth, 10);
        assert_eq!(cfg.pagination_cache_ttl_secs, 300);
        assert!(cfg.extra_ignore_patterns.is_empty());
    }

    #[test]
    fn test_orchestrator_defaults_match_hardcoded() {
        let cfg = OrchestratorConfig::default();
        assert_eq!(cfg.session_deadline_secs, 3600);
        assert_eq!(cfg.inactivity_timeout_secs, 300);
        assert!((cfg.compaction_threshold - 0.80).abs() < f64::EPSILON);
    }

    #[test]
    fn test_services_defaults_match_hardcoded() {
        let cfg = ServicesConfig::default();

        // Anthropic
        assert_eq!(cfg.anthropic.base_url, "https://api.anthropic.com");

        // Exa
        assert_eq!(cfg.exa.base_url, "https://api.exa.ai");
    }
}
