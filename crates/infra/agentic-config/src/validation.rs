//! Advisory validation for `AgenticConfig`.
//!
//! Validation is advisory - it produces warnings but doesn't prevent
//! the config from being used. This allows tools to work with imperfect
//! configs while still surfacing potential issues.

use crate::types::AgenticConfig;
use std::collections::BTreeSet;

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

    if let Some(tbl) = v.as_table() {
        if let Some(thoughts) = tbl.get("thoughts").and_then(toml::Value::as_table)
            && thoughts.contains_key("mount_dirs")
        {
            warnings.push(AdvisoryWarning::new(
                "config.deprecated.thoughts.mount_dirs",
                "thoughts.mount_dirs",
                "The legacy thoughts.mount_dirs key is no longer supported. The agentic [thoughts] section now only models add_reference_timeout_secs.",
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
    "workspace_tools",
    "review",
    "thoughts",
    "logging",
];

const GPT5_2_COMPLETION_TOKENS_DOC_MAX: u32 = 128_000;

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
                format!("Unknown top-level key '{key}' will be ignored"),
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
    validate_url(
        &cfg.services.linear.base_url,
        "services.linear.base_url",
        "services.linear.base_url.invalid",
        &mut warnings,
    );
    validate_url(
        &cfg.services.github.base_url,
        "services.github.base_url",
        "services.github.base_url.invalid",
        &mut warnings,
    );
    validate_url(
        &cfg.services.discord.base_url,
        "services.discord.base_url",
        "services.discord.base_url.invalid",
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
    validate_low_nonzero_timeout(
        cfg.subagents.runtime_timeout_secs,
        30,
        "subagents.runtime_timeout_secs",
        "subagents.runtime_timeout_secs.suspicious",
        &mut warnings,
    );

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

    if cfg
        .reasoning
        .executor_model
        .to_lowercase()
        .contains("gpt-5.2")
        && let Some(n) = cfg.reasoning.max_completion_tokens
        && n > GPT5_2_COMPLETION_TOKENS_DOC_MAX
    {
        warnings.push(AdvisoryWarning::new(
            "reasoning.max_completion_tokens.exceeds_doc",
            "reasoning.max_completion_tokens",
            format!(
                "max_completion_tokens={n} exceeds documented GPT-5.2 ceiling {GPT5_2_COMPLETION_TOKENS_DOC_MAX}; request may be rejected or truncate unexpectedly (warn-only; not clamped)."
            ),
        ));
    }

    if let Some(n) = cfg.reasoning.max_input_tokens
        && n > 250_000
    {
        warnings.push(AdvisoryWarning::new(
            "reasoning.max_input_tokens.suspicious",
            "reasoning.max_input_tokens",
            format!(
                "max_input_tokens={n} exceeds the tool's default prompt cap (250000); ensure executor model supports this context size (warn-only)."
            ),
        ));
    }

    // Validate orchestrator.compaction_threshold is in (0,1]
    if !(0.0..=1.0).contains(&cfg.orchestrator.compaction_threshold) {
        warnings.push(AdvisoryWarning::new(
            "orchestrator.compaction_threshold.out_of_range",
            "orchestrator.compaction_threshold",
            "expected a value between 0.0 and 1.0",
        ));
    }

    validate_command_entries(
        &cfg.orchestrator.commands.allow,
        "orchestrator.commands.allow",
        &mut warnings,
    );
    validate_command_entries(
        &cfg.orchestrator.commands.deny,
        "orchestrator.commands.deny",
        &mut warnings,
    );
    validate_command_overlap(cfg, &mut warnings);
    validate_agent_entries(
        &cfg.orchestrator.agents.allow,
        "orchestrator.agents.allow",
        &mut warnings,
    );
    validate_agent_entries(
        &cfg.orchestrator.agents.deny,
        "orchestrator.agents.deny",
        &mut warnings,
    );
    validate_agent_overlap(cfg, &mut warnings);

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

    validate_low_nonzero_timeout(
        cfg.cli_tools.just_execute_timeout_secs,
        5,
        "cli_tools.just_execute_timeout_secs",
        "cli_tools.just_execute_timeout_secs.suspicious",
        &mut warnings,
    );
    validate_low_nonzero_timeout(
        cfg.cli_tools.just_search_timeout_secs,
        2,
        "cli_tools.just_search_timeout_secs",
        "cli_tools.just_search_timeout_secs.suspicious",
        &mut warnings,
    );
    validate_low_nonzero_timeout(
        cfg.services.linear.connect_timeout_secs,
        1,
        "services.linear.connect_timeout_secs",
        "services.linear.connect_timeout_secs.suspicious",
        &mut warnings,
    );
    validate_low_nonzero_timeout(
        cfg.services.linear.request_timeout_secs,
        5,
        "services.linear.request_timeout_secs",
        "services.linear.request_timeout_secs.suspicious",
        &mut warnings,
    );
    validate_low_nonzero_timeout(
        cfg.services.github.total_timeout_secs,
        5,
        "services.github.total_timeout_secs",
        "services.github.total_timeout_secs.suspicious",
        &mut warnings,
    );
    validate_low_nonzero_timeout(
        cfg.services.discord.request_timeout_secs,
        5,
        "services.discord.request_timeout_secs",
        "services.discord.request_timeout_secs.suspicious",
        &mut warnings,
    );
    validate_low_nonzero_timeout(
        cfg.review.run_timeout_secs,
        30,
        "review.run_timeout_secs",
        "review.run_timeout_secs.suspicious",
        &mut warnings,
    );
    validate_low_nonzero_timeout(
        cfg.thoughts.add_reference_timeout_secs,
        5,
        "thoughts.add_reference_timeout_secs",
        "thoughts.add_reference_timeout_secs.suspicious",
        &mut warnings,
    );

    warnings
}

fn validate_low_nonzero_timeout(
    value: u64,
    minimum_recommended: u64,
    path: &'static str,
    code: &'static str,
    warnings: &mut Vec<AdvisoryWarning>,
) {
    if value != 0 && value < minimum_recommended {
        warnings.push(AdvisoryWarning::new(
            code,
            path,
            format!(
                "value {value}s is very low; {minimum_recommended}s or higher is usually safer, or use 0 to disable"
            ),
        ));
    }
}

fn validate_command_entries(
    entries: &[String],
    path: &'static str,
    warnings: &mut Vec<AdvisoryWarning>,
) {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();

    for entry in entries {
        let trimmed = entry.trim();

        if trimmed.is_empty() {
            warnings.push(AdvisoryWarning::new(
                if path.ends_with("allow") {
                    "orchestrator.commands.allow.empty_entry"
                } else {
                    "orchestrator.commands.deny.empty_entry"
                },
                path,
                format!("entry {entry:?} becomes empty after trimming"),
            ));
            continue;
        }

        if trimmed != entry {
            warnings.push(AdvisoryWarning::new(
                if path.ends_with("allow") {
                    "orchestrator.commands.allow.trimmed_entry"
                } else {
                    "orchestrator.commands.deny.trimmed_entry"
                },
                path,
                format!(
                    "entry {entry:?} has surrounding whitespace; effective value is {trimmed:?}"
                ),
            ));
        }

        if !seen.insert(trimmed.to_string()) {
            duplicates.insert(trimmed.to_string());
        }
    }

    if !duplicates.is_empty() {
        let duplicates = duplicates.into_iter().collect::<Vec<_>>().join(", ");
        warnings.push(AdvisoryWarning::new(
            if path.ends_with("allow") {
                "orchestrator.commands.allow.duplicate"
            } else {
                "orchestrator.commands.deny.duplicate"
            },
            path,
            format!("duplicate command entries after trimming: {duplicates}"),
        ));
    }
}

fn validate_command_overlap(cfg: &AgenticConfig, warnings: &mut Vec<AdvisoryWarning>) {
    let allow = cfg
        .orchestrator
        .commands
        .allow
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let deny = cfg
        .orchestrator
        .commands
        .deny
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();

    let overlap = allow.intersection(&deny).cloned().collect::<Vec<_>>();
    if overlap.is_empty() {
        return;
    }

    warnings.push(AdvisoryWarning::new(
        "orchestrator.commands.overlap",
        "orchestrator.commands",
        format!(
            "commands appear in both allow and deny: {}. deny wins at runtime",
            overlap.join(", ")
        ),
    ));
}

fn validate_agent_entries(
    entries: &[String],
    path: &'static str,
    warnings: &mut Vec<AdvisoryWarning>,
) {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();

    for entry in entries {
        let trimmed = entry.trim();

        if trimmed.is_empty() {
            warnings.push(AdvisoryWarning::new(
                if path.ends_with("allow") {
                    "orchestrator.agents.allow.empty_entry"
                } else {
                    "orchestrator.agents.deny.empty_entry"
                },
                path,
                format!("entry {entry:?} becomes empty after trimming"),
            ));
            continue;
        }

        if trimmed != entry {
            warnings.push(AdvisoryWarning::new(
                if path.ends_with("allow") {
                    "orchestrator.agents.allow.trimmed_entry"
                } else {
                    "orchestrator.agents.deny.trimmed_entry"
                },
                path,
                format!(
                    "entry {entry:?} has surrounding whitespace; effective value is {trimmed:?}"
                ),
            ));
        }

        if !seen.insert(trimmed.to_string()) {
            duplicates.insert(trimmed.to_string());
        }
    }

    if !duplicates.is_empty() {
        let duplicates = duplicates.into_iter().collect::<Vec<_>>().join(", ");
        warnings.push(AdvisoryWarning::new(
            if path.ends_with("allow") {
                "orchestrator.agents.allow.duplicate"
            } else {
                "orchestrator.agents.deny.duplicate"
            },
            path,
            format!("duplicate agent entries after trimming: {duplicates}"),
        ));
    }
}

fn validate_agent_overlap(cfg: &AgenticConfig, warnings: &mut Vec<AdvisoryWarning>) {
    let allow = cfg
        .orchestrator
        .agents
        .allow
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let deny = cfg
        .orchestrator
        .agents
        .deny
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();

    let overlap = allow.intersection(&deny).cloned().collect::<Vec<_>>();
    if overlap.is_empty() {
        return;
    }

    warnings.push(AdvisoryWarning::new(
        "orchestrator.agents.overlap",
        "orchestrator.agents",
        format!(
            "agents appear in both allow and deny: {}. deny wins at runtime",
            overlap.join(", ")
        ),
    ));
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
            message: format!("Expected an http(s) URL, got: '{url}'"),
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
            "Default config should have no warnings: {warnings:?}"
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
    fn test_invalid_linear_and_github_urls_warn() {
        let mut config = AgenticConfig::default();
        config.services.linear.base_url = "linear".into();
        config.services.github.base_url = "github".into();
        config.services.discord.base_url = "discord".into();

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "services.linear.base_url.invalid")
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "services.github.base_url.invalid")
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "services.discord.base_url.invalid")
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
        let display = format!("{warning}");
        assert_eq!(display, "[test.code] test.path: Test message");
    }

    #[test]
    fn test_empty_subagent_model_warns() {
        let mut config = AgenticConfig::default();
        config.subagents.locator_model = String::new();

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
    fn test_orchestrator_allow_empty_entry_warns() {
        let mut config = AgenticConfig::default();
        config.orchestrator.commands.allow = vec!["   ".into()];

        let warnings = validate(&config);
        let warning = warnings
            .iter()
            .find(|w| w.code == "orchestrator.commands.allow.empty_entry")
            .expect("empty allow warning expected");

        assert_eq!(warning.path, "orchestrator.commands.allow");
        assert!(warning.message.contains("becomes empty after trimming"));
    }

    #[test]
    fn test_orchestrator_deny_trimmed_entry_warns() {
        let mut config = AgenticConfig::default();
        config.orchestrator.commands.deny = vec!["  plan  ".into()];

        let warnings = validate(&config);
        let warning = warnings
            .iter()
            .find(|w| w.code == "orchestrator.commands.deny.trimmed_entry")
            .expect("trimmed deny warning expected");

        assert_eq!(warning.path, "orchestrator.commands.deny");
        assert!(warning.message.contains("effective value is \"plan\""));
    }

    #[test]
    fn test_orchestrator_allow_duplicate_warns() {
        let mut config = AgenticConfig::default();
        config.orchestrator.commands.allow = vec!["plan".into(), " plan ".into()];

        let warnings = validate(&config);
        let warning = warnings
            .iter()
            .find(|w| w.code == "orchestrator.commands.allow.duplicate")
            .expect("duplicate allow warning expected");

        assert_eq!(warning.path, "orchestrator.commands.allow");
        assert!(warning.message.contains("plan"));
    }

    #[test]
    fn test_orchestrator_command_overlap_warns_with_deny_wins_message() {
        let mut config = AgenticConfig::default();
        config.orchestrator.commands.allow = vec!["plan".into()];
        config.orchestrator.commands.deny = vec![" plan ".into()];

        let warnings = validate(&config);
        let warning = warnings
            .iter()
            .find(|w| w.code == "orchestrator.commands.overlap")
            .expect("overlap warning expected");

        assert_eq!(warning.path, "orchestrator.commands");
        assert!(warning.message.contains("plan"));
        assert!(warning.message.contains("deny wins at runtime"));
    }

    #[test]
    fn test_orchestrator_agents_allow_empty_entry_warns() {
        let mut config = AgenticConfig::default();
        config.orchestrator.agents.allow = vec!["   ".into()];

        let warnings = validate(&config);
        let warning = warnings
            .iter()
            .find(|w| w.code == "orchestrator.agents.allow.empty_entry")
            .expect("empty allow warning expected");

        assert_eq!(warning.path, "orchestrator.agents.allow");
        assert!(warning.message.contains("becomes empty after trimming"));
    }

    #[test]
    fn test_orchestrator_agents_deny_trimmed_entry_warns() {
        let mut config = AgenticConfig::default();
        config.orchestrator.agents.deny = vec!["  Bash  ".into()];

        let warnings = validate(&config);
        let warning = warnings
            .iter()
            .find(|w| w.code == "orchestrator.agents.deny.trimmed_entry")
            .expect("trimmed deny warning expected");

        assert_eq!(warning.path, "orchestrator.agents.deny");
        assert!(warning.message.contains("effective value is \"Bash\""));
    }

    #[test]
    fn test_orchestrator_agents_allow_duplicate_warns() {
        let mut config = AgenticConfig::default();
        config.orchestrator.agents.allow = vec!["Bash".into(), " Bash ".into()];

        let warnings = validate(&config);
        let warning = warnings
            .iter()
            .find(|w| w.code == "orchestrator.agents.allow.duplicate")
            .expect("duplicate allow warning expected");

        assert_eq!(warning.path, "orchestrator.agents.allow");
        assert!(warning.message.contains("Bash"));
    }

    #[test]
    fn test_orchestrator_agent_overlap_warns_with_deny_wins_message() {
        let mut config = AgenticConfig::default();
        config.orchestrator.agents.allow = vec!["Bash".into()];
        config.orchestrator.agents.deny = vec![" Bash ".into()];

        let warnings = validate(&config);
        let warning = warnings
            .iter()
            .find(|w| w.code == "orchestrator.agents.overlap")
            .expect("overlap warning expected");

        assert_eq!(warning.path, "orchestrator.agents");
        assert!(warning.message.contains("Bash"));
        assert!(warning.message.contains("deny wins at runtime"));
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
            r"
[thoughts]
mount_dirs = {}
",
        )
        .unwrap();

        let warnings = detect_deprecated_keys_toml(&toml_val);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "config.deprecated.thoughts.mount_dirs")
        );
    }

    #[test]
    fn test_supported_thoughts_section_is_not_deprecated() {
        let toml_val: toml::Value = toml::from_str(
            r"
[thoughts]
add_reference_timeout_secs = 600
",
        )
        .unwrap();

        let warnings = detect_deprecated_keys_toml(&toml_val);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_detect_deprecated_reasoning_token_limit_toml_is_silent() {
        let toml_val: toml::Value = toml::from_str(
            r"
[reasoning]
token_limit = 12345
",
        )
        .unwrap();

        let warnings = detect_deprecated_keys_toml(&toml_val);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_reasoning_max_completion_tokens_above_doc_max_warns() {
        let mut config = AgenticConfig::default();
        config.reasoning.max_completion_tokens = Some(128_001);

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "reasoning.max_completion_tokens.exceeds_doc")
        );
    }

    #[test]
    fn test_reasoning_max_input_tokens_above_default_cap_warns() {
        let mut config = AgenticConfig::default();
        config.reasoning.max_input_tokens = Some(250_001);

        let warnings = validate(&config);
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "reasoning.max_input_tokens.suspicious")
        );
    }

    #[test]
    fn test_low_nonzero_timeout_values_warn() {
        let mut config = AgenticConfig::default();
        config.subagents.runtime_timeout_secs = 1;
        config.cli_tools.just_execute_timeout_secs = 1;
        config.cli_tools.just_search_timeout_secs = 1;
        config.services.linear.request_timeout_secs = 1;
        config.services.github.total_timeout_secs = 1;
        config.services.discord.request_timeout_secs = 1;
        config.review.run_timeout_secs = 1;
        config.thoughts.add_reference_timeout_secs = 1;

        let warnings = validate(&config);
        for code in [
            "subagents.runtime_timeout_secs.suspicious",
            "cli_tools.just_execute_timeout_secs.suspicious",
            "cli_tools.just_search_timeout_secs.suspicious",
            "services.linear.request_timeout_secs.suspicious",
            "services.github.total_timeout_secs.suspicious",
            "services.discord.request_timeout_secs.suspicious",
            "review.run_timeout_secs.suspicious",
            "thoughts.add_reference_timeout_secs.suspicious",
        ] {
            assert!(warnings.iter().any(|w| w.code == code), "missing {code}");
        }
    }
}
