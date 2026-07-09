use opencode_orchestrator_mcp::server::OrchestratorServerHandle;
use opencode_orchestrator_mcp::tools::build_registry;
use serde_json::Value;
use std::sync::Arc;

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|error| format!("<failed to format json: {error}>"))
}

#[test]
fn respond_permission_reply_schema_is_inline_string_enum() {
    let server = Arc::new(OrchestratorServerHandle::new());
    let registry = build_registry(&server);
    let tool = registry
        .get("respond_permission")
        .expect("respond_permission tool must be registered");

    let schema = serde_json::to_value(tool.input_schema())
        .expect("respond_permission input schema should serialize to JSON");
    let reply = &schema["properties"]["reply"];

    assert!(
        schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|value| value == "reply"),
        "reply must be required in respond_permission schema. Full schema:\n{}",
        pretty(&schema)
    );

    for forbidden_key in ["$ref", "anyOf", "oneOf", "allOf"] {
        assert!(
            reply.get(forbidden_key).is_none(),
            "reply schema must be inline, not behind {forbidden_key}. Reply fragment:\n{}",
            pretty(reply)
        );
    }

    assert_eq!(
        reply.get("type"),
        Some(&serde_json::json!("string")),
        "reply schema must be a string enum. Reply fragment:\n{}",
        pretty(reply)
    );
    assert_eq!(
        reply.get("enum"),
        Some(&serde_json::json!(["once", "always", "reject"])),
        "reply schema must expose the expected enum values. Reply fragment:\n{}",
        pretty(reply)
    );
}
