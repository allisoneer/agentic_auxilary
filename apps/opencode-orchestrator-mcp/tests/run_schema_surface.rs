use opencode_orchestrator_mcp::server::OrchestratorServerHandle;
use opencode_orchestrator_mcp::tools::build_registry;
use std::sync::Arc;

#[test]
fn run_input_schema_does_not_advertise_wait_for_activity() {
    let handle = Arc::new(OrchestratorServerHandle::new());
    let registry = build_registry(&handle);

    let Some(run_tool) = registry.get("run") else {
        panic!("run tool must be registered");
    };
    let schema = run_tool.input_schema();
    let json = match serde_json::to_value(&schema) {
        Ok(value) => value,
        Err(error) => panic!("schema must serialize: {error}"),
    };
    let json_str = match serde_json::to_string(&json) {
        Ok(value) => value,
        Err(error) => panic!("schema JSON must serialize: {error}"),
    };

    assert!(
        !json_str.contains("wait_for_activity"),
        "run inputSchema must not contain wait_for_activity; got: {json_str}"
    );
}
