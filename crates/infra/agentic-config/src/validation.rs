//! Advisory validation for AgenticConfig.
//!
//! Validation is advisory - it produces warnings but doesn't prevent
//! the config from being used. This allows tools to work with imperfect
//! configs while still surfacing potential issues.

use crate::types::AgenticConfig;

/// An advisory warning about a configuration issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisoryWarning {
    /// Machine-readable warning code.
    pub code: &'static str,

    /// Human-readable warning message.
    pub message: String,

    /// Config path to the problematic field.
    pub path: &'static str,
}

impl AdvisoryWarning {
    /// Create a new advisory warning.
    pub fn new(code: &'static str, path: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            path,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AdvisoryWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.code, self.path, self.message)
    }
}

/// Detect deprecated config keys in raw TOML before deserialization.
///
/// This inspects the merged TOML Value to detect keys that are no longer
/// used and emit advisory warnings. The config will still load successfully,
/// but users will be notified that they should update their configuration.
pub fn detect_deprecated_keys_toml(v: &toml::Value) -> Vec<AdvisoryWarning> {
    let mut warnings = Vec::new();

    // Warn if old "thoughts" section exists (removed in this version)
    if let Some(tbl) = v.as_table() {
        if tbl.contains_key("thoughts") {
            warnings.push(AdvisoryWarning::new(
                "config.deprecated.thoughts",
                "thoughts",
                "The 'thoughts' section has been removed. thoughts-core now has its own config.",
            ));
        }
        if tbl.contains_key("models") {
            warnings.push(AdvisoryWarning::new(
                "config.deprecated.models",
                "models",
                "The 'models' section has been replaced by 'subagents' and 'reasoning'.",
            ));
        }
    }

    warnings
}

// TODO(2): This list must be kept in sync with AgenticConfig fields in types.rs.
// Consider generating dynamically via schemars introspection, or adding a compile-time
// test that extracts field names from AgenticConfig's JsonSchema and verifies they
// match this list. Currently requires manual updates when adding new config sections.
// See research/pr127-group7-type-safety-external-type-dependencies.md for analysis.

/// Known top-level keys for unknown key detection.
/// Unknown keys at root level produce advisory warnings.
const KNOWN_TOP_LEVEL_KEYS: &[&str] = &[
    "$schema",
    "subagents",
    "reasoning",
    "services",
    "orchestrator",
    "web_retrieval",
    "cli_tools",
    "logging",
];

/// Detect unknown top-level keys in raw TOML before deserialization.
///
/// Unknown keys at the root are ignored by serde, so we emit an advisory warning
/// to help users catch typos like `[servics]` instead of `[services]`.
pub fn detect_unknown_top_level_keys_toml(v: &toml::Value) -> Vec<AdvisoryWarning> {
    let mut warnings = Vec::new();
    let Some(tbl) = v.as_table() else {
        return warnings;
    };

    for key in tbl.keys() {
        if !KNOWN_TOP_LEVEL_KEYS.contains(&key.as_str()) {
            warnings.push(AdvisoryWarning::new(
                "config.unknown_top_level_key",
                "$",
                format!("Unknown top-level key '{}' will be ignored", key),
            ));
        }
    }

    warnings
}

/// Validate a configuration and return advisory warnings.
///
/// This does NOT fail on issues - it only collects warnings that
/// callers can choose to display or log.
pub fn validate(cfg: &AgenticConfig) -> Vec<AdvisoryWarning> {
    let mut warnings = vec![];

    // Validate service URLs
    validate_url(
        &cfg.services.anthropic.base_url,
        "services.anthropic.base_url",
        "services.anthropic.base_url.invalid",
        &mut warnings,
    );

    validate_url(
        &cfg.services.exa.base_url,
        "services.exa.base_url",
        "services.exa.base_url.invalid",
        &mut warnings,
    );

    // Validate log level
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_levels.contains(&cfg.logging.level.to_lowercase().as_str()) {
        warnings.push(AdvisoryWarning {
            code: "logging.level.invalid",
            path: "logging.level",
            message: format!(
                "Unknown log level '{}'. Expected one of: {}",
                cfg.logging.level,
                valid_levels.join(", ")
            ),
        });
    }

    // Validate subagents model values are not empty
    if cfg.subagents.locator_model.trim().is_empty() {
        warnings.push(AdvisoryWarning::new(
            "subagents.locator_model.empty",
            "subagents.locator_model",
            "value is empty",
        ));
    }
    if cfg.subagents.analyzer_model.trim().is_empty() {
        warnings.push(AdvisoryWarning::new(
            "subagents.analyzer_model.empty",
            "subagents.analyzer_model",
            "value is empty",
        ));
    }

    // Validate reasoning model values are not empty
    if cfg.reasoning.optimizer_model.trim().is_empty() {
        warnings.push(AdvisoryWarning::new(
            "reasoning.optimizer_model.empty",
            "reasoning.optimizer_model",
            "value is empty",
        ));
    }
    if cfg.reasoning.executor_model.trim().is_empty() {
        warnings.push(AdvisoryWarning::new(
            "reasoning.executor_model.empty",
            "reasoning.executor_model",
            "value is empty",
        ));
    }

    // Validate OpenRouter format for reasoning models (should contain '/')
    if !cfg.reasoning.optimizer_model.trim().is_empty()
        && !cfg.reasoning.optimizer_model.contains('/')
    {
        warnings.push(AdvisoryWarning::new(
            "reasoning.optimizer_model.format",
            "reasoning.optimizer_model",
            "expected OpenRouter format like `anthropic/claude-sonnet-4.6`",
        ));
    }

    if !cfg.reasoning.executor_model.trim().is_empty()
        && !cfg.reasoning.executor_model.contains('/')
    {
        warnings.push(AdvisoryWarning::new(
            "reasoning.executor_model.format",
            "reasoning.executor_model",
            "expected OpenRouter format like `openai/gpt-5.2`",
        ));
    } else if !cfg.reasoning.executor_model.trim().is_empty()
        && !cfg
            .reasoning
            .executor_model
            .to_lowercase()
            .contains("gpt-5")
    {
        warnings.push(AdvisoryWarning::new(
            "reasoning.executor_model.suspicious",
            "reasoning.executor_model",
            "executor_model does not look like a GPT-5 model; reasoning_effort may not work",
        ));
    }

    // Validate reasoning_effort enum
    if let Some(eff) = cfg.reasoning.reasoning_effort.as_deref() {
        let eff_lc = eff.trim().to_lowercase();
        if !matches!(eff_lc.as_str(), "low" | "medium" | "high" | "xhigh") {
            warnings.push(AdvisoryWarning::new(
                "reasoning.reasoning_effort.invalid",
                "reasoning.reasoning_effort",
                "expected one of: low, medium, high, xhigh",
            ));
        }
    }

    // Validate orchestrator.compaction_threshold is in (0,1]
    if !(0.0..=1.0).contains(&cfg.orchestrator.compaction_threshold) {
        warnings.push(AdvisoryWarning::new(
            "orchestrator.compaction_threshold.out_of_range",
            "orchestrator.compaction_threshold",
            "expected a value between 0.0 and 1.0",
        ));
    }

    // Validate web_retrieval: default_search_results <= max_search_results
    if cfg.web_retrieval.default_search_results > cfg.web_retrieval.max_search_results {
        warnings.push(AdvisoryWarning::new(
            "web_retrieval.default_exceeds_max",
            "web_retrieval.default_search_results",
            "default_search_results exceeds max_search_results",
        ));
    }

    // Validate web_retrieval.summarizer.model is not empty
    if cfg.web_retrieval.summarizer.model.trim().is_empty() {
        warnings.push(AdvisoryWarning::new(
            "web_retrieval.summarizer.model.empty",
            "web_retrieval.summarizer.model",
            "value is empty",
        ));
    }

    // Validate cli_tools.max_depth is reasonable
    if cfg.cli_tools.max_depth == 0 {
        warnings.push(AdvisoryWarning::new(
            "cli_tools.max_depth.zero",
            "cli_tools.max_depth",
            "max_depth is 0, directory listing may be limited",
        ));
    }

    warnings
}

fn validate_url(
    url: &str,
    path: &'static str,
    code: &'static str,
    warnings: &mut Vec<AdvisoryWarning>,
) {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        warnings.push(AdvisoryWarning {
            code,
            path,
            message: format!("Expected an http(s) URL, got: '{}'", url),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_no_warnings() {
        let config = AgenticConfig::default();
        let warnings = validate(&config);
        assert!(
            warnings.is_empty(),
            "Default config should have no warnings: {:?}",
            warnings
        );
    }

    #[test]
    fn test_invalid_anthropic_url_warns() {
        let mut config = AgenticConfig::default();
        config.services.anthropic.base_url = "not-a-url".into();

        let warnings = validate(&config);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "services.anthropic.base_url.invalid");
    }

    #[test]
    fn test_invalid_log_level_warns() {
        let mut config = AgenticConfig::default();
        config.logging.level = "verbose".into();

        let warnings = validate(&config);
        assert!(warnings.iter().any(|w| w.code == "logging.level.invalid"));
    }

    #[test]
    fn test_warning_display() {
        let warning = AdvisoryWarning {
            code: "test.code",
            path: "test.path",
            message: "Test message".into(),
        };
        let display = format!("{}", warning);
        assert_eq!(display, "[test.code] test.path: Test message");
    }

    #[test]
    fn test_empty_subagent_model_warns() {
        let mut config = AgenticConfig::default();
        config.subagents.locator_model = "".into();

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "subagents.locator_model.empty")
        );
    }

    #[test]
    fn test_reasoning_optimizer_model_format_warns() {
        let mut config = AgenticConfig::default();
        config.reasoning.optimizer_model = "claude-sonnet-4.6".into(); // Missing provider prefix

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "reasoning.optimizer_model.format")
        );
    }

    #[test]
    fn test_reasoning_executor_model_suspicious_warns() {
        let mut config = AgenticConfig::default();
        config.reasoning.executor_model = "anthropic/claude-sonnet-4.6".into(); // Not GPT-5

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "reasoning.executor_model.suspicious")
        );
    }

    #[test]
    fn test_reasoning_effort_invalid_warns() {
        let mut config = AgenticConfig::default();
        config.reasoning.reasoning_effort = Some("extreme".into()); // Invalid value

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "reasoning.reasoning_effort.invalid")
        );
    }

    #[test]
    fn test_reasoning_effort_valid_no_warning() {
        let mut config = AgenticConfig::default();
        config.reasoning.reasoning_effort = Some("high".into());

        let warnings = validate(&config);
        assert!(
            !warnings
                .iter()
                .any(|w| w.code == "reasoning.reasoning_effort.invalid")
        );
    }

    #[test]
    fn test_orchestrator_compaction_threshold_out_of_range() {
        let mut config = AgenticConfig::default();
        config.orchestrator.compaction_threshold = 1.5; // Invalid

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "orchestrator.compaction_threshold.out_of_range")
        );
    }

    #[test]
    fn test_web_retrieval_default_exceeds_max() {
        let mut config = AgenticConfig::default();
        config.web_retrieval.default_search_results = 100;
        config.web_retrieval.max_search_results = 20;

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "web_retrieval.default_exceeds_max")
        );
    }

    #[test]
    fn test_detect_deprecated_thoughts_toml() {
        let toml_val: toml::Value = toml::from_str(
            r#"
[thoughts]
mount_dirs = {}
"#,
        )
        .unwrap();

        let warnings = detect_deprecated_keys_toml(&toml_val);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "config.deprecated.thoughts")
        );
    }
}
