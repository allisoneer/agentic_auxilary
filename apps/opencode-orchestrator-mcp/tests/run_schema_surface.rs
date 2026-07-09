#![allow(clippy::expect_used, clippy::unwrap_used)]

use opencode_orchestrator_mcp::server::OrchestratorServerHandle;
use opencode_orchestrator_mcp::tools::build_registry;
use std::sync::Arc;

#[test]
fn run_input_schema_does_not_advertise_wait_for_activity() {
    let handle = Arc::new(OrchestratorServerHandle::new());
    let registry = build_registry(&handle);

    let run_tool = registry.get("run").expect("run tool must be registered");
    let schema = run_tool.input_schema();
    let json = serde_json::to_value(&schema).expect("schema must serialize");
    let json_str = serde_json::to_string(&json).expect("schema JSON must serialize");

    assert!(
        !json_str.contains("wait_for_activity"),
        "run inputSchema must not contain wait_for_activity; got: {json_str}"
    );
}
