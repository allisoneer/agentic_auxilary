//! Advisory validation for AgenticConfig.
//!
//! Validation is advisory - it produces warnings but doesn't prevent
//! the config from being used. This allows tools to work with imperfect
//! configs while still surfacing potential issues.

use crate::types::AgenticConfig;
use serde_json::Value;

/// An advisory warning about a configuration issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisoryWarning {
    /// Machine-readable warning code.
    pub code: &'static str,

    /// Human-readable warning message.
    pub message: String,

    /// JSON path to the problematic config field.
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

/// Detect deprecated config keys in raw JSON before deserialization.
///
/// This inspects the merged JSON Value to detect keys that are no longer
/// used and emit advisory warnings. The config will still load successfully,
/// but users will be notified that they should update their configuration.
pub fn detect_deprecated_keys(_v: &Value) -> Vec<AdvisoryWarning> {
    // Intentionally empty: reserved for future schema deprecations.
    Vec::new()
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

    // Validate mount directories are non-empty
    validate_non_empty(
        &cfg.thoughts.mount_dirs.thoughts,
        "thoughts.mount_dirs.thoughts",
        "thoughts.mount_dirs.thoughts.empty",
        &mut warnings,
    );

    validate_non_empty(
        &cfg.thoughts.mount_dirs.context,
        "thoughts.mount_dirs.context",
        "thoughts.mount_dirs.context.empty",
        &mut warnings,
    );

    validate_non_empty(
        &cfg.thoughts.mount_dirs.references,
        "thoughts.mount_dirs.references",
        "thoughts.mount_dirs.references.empty",
        &mut warnings,
    );

    // Validate mount directories are distinct
    let dirs = &cfg.thoughts.mount_dirs;
    if dirs.thoughts == dirs.context {
        warnings.push(AdvisoryWarning {
            code: "thoughts.mount_dirs.duplicate",
            path: "thoughts.mount_dirs",
            message: format!(
                "thoughts and context directories are the same: '{}'",
                dirs.thoughts
            ),
        });
    }
    if dirs.thoughts == dirs.references {
        warnings.push(AdvisoryWarning {
            code: "thoughts.mount_dirs.duplicate",
            path: "thoughts.mount_dirs",
            message: format!(
                "thoughts and references directories are the same: '{}'",
                dirs.thoughts
            ),
        });
    }
    if dirs.context == dirs.references {
        warnings.push(AdvisoryWarning {
            code: "thoughts.mount_dirs.duplicate",
            path: "thoughts.mount_dirs",
            message: format!(
                "context and references directories are the same: '{}'",
                dirs.context
            ),
        });
    }

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

fn validate_non_empty(
    value: &str,
    path: &'static str,
    code: &'static str,
    warnings: &mut Vec<AdvisoryWarning>,
) {
    if value.trim().is_empty() {
        warnings.push(AdvisoryWarning {
            code,
            path,
            message: "Value cannot be empty".into(),
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
    fn test_empty_mount_dir_warns() {
        let mut config = AgenticConfig::default();
        config.thoughts.mount_dirs.thoughts = "".into();

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "thoughts.mount_dirs.thoughts.empty")
        );
    }

    #[test]
    fn test_duplicate_mount_dirs_warn() {
        let mut config = AgenticConfig::default();
        config.thoughts.mount_dirs.thoughts = "same".into();
        config.thoughts.mount_dirs.context = "same".into();

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "thoughts.mount_dirs.duplicate")
        );
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
}
