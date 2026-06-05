//! JSON Schema generation for `AgenticConfig`.
//!
//! Uses schemars to generate a JSON Schema that can be used for
//! IDE autocomplete and validation.

use crate::types::AgenticConfig;
use schemars::Schema;
use schemars::generate::SchemaSettings;

/// Generate the JSON Schema for `AgenticConfig`.
pub fn schema() -> Schema {
    SchemaSettings::default()
        .into_generator()
        .into_root_schema_for::<AgenticConfig>()
}

/// Generate the JSON Schema as a pretty-printed JSON string.
pub fn schema_json_pretty() -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(&schema())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_is_valid_json() {
        let json = schema_json_pretty().unwrap();
        let _: serde_json::Value = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_schema_has_required_properties() {
        let schema = schema();
        let json = serde_json::to_value(&schema).unwrap();

        // Check that the schema has definitions for our types
        assert!(json.get("$defs").is_some() || json.get("definitions").is_some());

        let orchestrator_properties = &json["$defs"]["OrchestratorConfig"]["properties"];
        assert!(orchestrator_properties.get("commands").is_some());
        assert!(orchestrator_properties.get("agents").is_some());

        let command_properties = &json["$defs"]["OrchestratorCommandsConfig"]["properties"];
        assert!(command_properties.get("allow").is_some());
        assert!(command_properties.get("deny").is_some());

        let agent_properties = &json["$defs"]["OrchestratorAgentsConfig"]["properties"];
        assert!(agent_properties.get("allow").is_some());
        assert!(agent_properties.get("deny").is_some());

        let subagents_properties = &json["$defs"]["SubagentsConfig"]["properties"];
        assert!(subagents_properties.get("runtime_timeout_secs").is_some());

        let cli_tools_properties = &json["$defs"]["CliToolsConfig"]["properties"];
        assert!(
            cli_tools_properties
                .get("just_execute_timeout_secs")
                .is_some()
        );
        assert!(
            cli_tools_properties
                .get("just_search_timeout_secs")
                .is_some()
        );

        let services_properties = &json["$defs"]["ServicesConfig"]["properties"];
        assert!(services_properties.get("linear").is_some());
        assert!(services_properties.get("github").is_some());

        let review_properties = &json["$defs"]["ReviewConfig"]["properties"];
        assert!(review_properties.get("run_timeout_secs").is_some());

        let thoughts_properties = &json["$defs"]["ThoughtsConfig"]["properties"];
        assert!(
            thoughts_properties
                .get("add_reference_timeout_secs")
                .is_some()
        );
    }

    #[test]
    fn test_schema_excludes_secrets() {
        let json = schema_json_pretty().unwrap();

        // API keys should not appear in schema (they're skipped via #[schemars(skip)])
        assert!(!json.contains("\"api_key\""));
    }

    #[test]
    fn test_default_config_validates_against_schema() {
        let schema = schema();
        let config = AgenticConfig::default();
        let config_json = serde_json::to_value(&config).unwrap();

        // Use jsonschema crate to validate
        let validator = jsonschema::validator_for(&serde_json::to_value(&schema).unwrap()).unwrap();
        let result = validator.validate(&config_json);

        assert!(
            result.is_ok(),
            "Default config should validate against schema: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_partial_config_validates_against_schema() {
        let schema = schema();
        let config_json = serde_json::json!({
            "subagents": {
                "locator_model": "claude-haiku-4-5"
            }
        });

        let validator = jsonschema::validator_for(&serde_json::to_value(&schema).unwrap()).unwrap();
        let result = validator.validate(&config_json);

        assert!(
            result.is_ok(),
            "Partial config should validate against schema: {:?}",
            result.err()
        );
    }
}
