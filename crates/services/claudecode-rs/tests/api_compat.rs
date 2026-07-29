use claudecode::Content;
use claudecode::Event;
use claudecode::MCPServer;
use claudecode::Session;
use std::collections::HashMap;
use tokio::sync::mpsc;

#[test]
fn content_0_1_24_variant_field_shapes_still_construct_and_match() {
    let tool_use = Content::ToolUse {
        id: "call-1".to_string(),
        name: "Read".to_string(),
        input: HashMap::from([("path".to_string(), serde_json::json!("README.md"))]),
    };
    let Content::ToolUse { id, name, input } = tool_use else {
        panic!("expected legacy tool-use shape");
    };
    assert_eq!(id, "call-1");
    assert_eq!(name, "Read");
    assert_eq!(input["path"], "README.md");

    let tool_result = Content::ToolResult {
        tool_use_id: "call-1".to_string(),
        content: "ok".to_string(),
    };
    let Content::ToolResult {
        tool_use_id,
        content,
    } = tool_result
    else {
        panic!("expected legacy tool-result shape");
    };
    assert_eq!(tool_use_id, "call-1");
    assert_eq!(content, "ok");
}

#[test]
fn mcp_server_0_1_24_construction_and_patterns_still_compile() {
    let stdio = MCPServer::Stdio {
        command: "server".to_string(),
        args: vec!["--stdio".to_string()],
        env: None,
    };
    let MCPServer::Stdio { command, args, env } = stdio else {
        panic!("expected stdio server");
    };
    assert_eq!(command, "server");
    assert_eq!(args, ["--stdio"]);
    assert!(env.is_none());

    let http = MCPServer::Http {
        url: "https://example.com/mcp".to_string(),
        headers: None,
    };
    let MCPServer::Http { url, headers } = http else {
        panic!("expected HTTP server");
    };
    assert_eq!(url, "https://example.com/mcp");
    assert!(headers.is_none());
}

#[expect(dead_code, reason = "compile-time public receiver contract assertion")]
fn take_event_stream_0_1_24_receiver_contract(
    session: &mut Session,
) -> Option<mpsc::UnboundedReceiver<Event>> {
    session.take_event_stream()
}
