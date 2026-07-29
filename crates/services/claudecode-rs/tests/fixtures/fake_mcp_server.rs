use serde_json::Value;
use serde_json::json;
use std::io::BufRead;
use std::io::Write;

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let response = match method {
            "initialize" => json!({
                "jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "fake-nonce", "version": "1.0.0"}
                }
            }),
            "tools/list" => json!({"jsonrpc": "2.0", "id": id, "result": {"tools": [{
                "name": "echo_nonce",
                "description": "Return the supplied nonce",
                "inputSchema": {
                    "additionalProperties": false,
                    "type": "object",
                    "properties": {"nonce": {"type": "string"}},
                    "required": ["nonce"]
                }
            }]}}),
            "tools/call" => {
                let name = request.pointer("/params/name").and_then(Value::as_str);
                let arguments = request.pointer("/params/arguments");
                let nonce = arguments
                    .and_then(Value::as_object)
                    .filter(|arguments| arguments.len() == 1)
                    .and_then(|arguments| arguments.get("nonce"))
                    .and_then(Value::as_str)
                    .filter(|nonce| !nonce.is_empty());
                match (name, nonce) {
                    (Some("echo_nonce"), Some(nonce)) => {
                        if let Ok(path) = std::env::var("FAKE_MCP_EVIDENCE_FILE") {
                            let _ = std::fs::write(path, nonce);
                        }
                        json!({"jsonrpc": "2.0", "id": id, "result": {
                            "content": [{"type": "text", "text": nonce}], "isError": false
                        }})
                    }
                    (Some("echo_nonce"), None) => json!({"jsonrpc": "2.0", "id": id, "error": {
                        "code": -32602, "message": "nonce must be one non-empty string argument"
                    }}),
                    _ => json!({"jsonrpc": "2.0", "id": id, "error": {
                        "code": -32601, "message": "unknown tool"
                    }}),
                }
            }
            _ => json!({"jsonrpc": "2.0", "id": id, "error": {
                "code": -32601, "message": "method not found"
            }}),
        };
        if writeln!(stdout, "{response}").is_err() || stdout.flush().is_err() {
            break;
        }
    }
}
