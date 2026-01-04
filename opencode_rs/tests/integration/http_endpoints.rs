//! HTTP endpoint integration tests.
//!
//! Tests that verify typed HTTP responses against a live opencode server.

use super::{create_test_client, should_run};
use opencode_rs::types::message::{PromptPart, PromptRequest};

/// Test session CRUD with typed responses.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_session_crud_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Create session - returns typed Session
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    assert!(!session.id.is_empty(), "Session should have ID");

    // Get session - returns typed Session
    let fetched = client
        .sessions()
        .get(&session.id)
        .await
        .expect("Failed to get session");

    assert_eq!(fetched.id, session.id, "Session IDs should match");

    // List sessions - returns Vec<Session>
    let sessions = client
        .sessions()
        .list()
        .await
        .expect("Failed to list sessions");

    assert!(
        sessions.iter().any(|s| s.id == session.id),
        "Created session should be in list"
    );

    // Delete session
    client
        .sessions()
        .delete(&session.id)
        .await
        .expect("Failed to delete session");
}

/// Test prompt with typed response.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_prompt_typed_response() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Create session
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    // Send prompt - returns typed PromptResponse
    let response = client
        .messages()
        .prompt(
            &session.id,
            &PromptRequest {
                parts: vec![PromptPart::Text {
                    text: "Say hello".to_string(),
                    synthetic: None,
                    ignored: None,
                    metadata: None,
                }],
                message_id: None,
                model: None,
                agent: None,
                no_reply: Some(true), // Don't wait for reply
                system: None,
                variant: None,
            },
        )
        .await
        .expect("Failed to send prompt");

    // PromptResponse has typed fields
    // status and message_id are optional but should deserialize
    println!("Prompt response status: {:?}", response.status);
    println!("Prompt response message_id: {:?}", response.message_id);

    // Clean up
    let _ = client.sessions().delete(&session.id).await;
}

/// Test providers list with typed response.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_providers_list_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // List providers - returns Vec<Provider>
    let providers = client
        .providers()
        .list()
        .await
        .expect("Failed to list providers");

    // Verify typed fields
    for provider in &providers {
        assert!(!provider.id.is_empty(), "Provider should have ID");
        assert!(!provider.name.is_empty(), "Provider should have name");
        println!(
            "Provider: {} ({}) - {:?} models",
            provider.name,
            provider.id,
            provider.models.len()
        );
    }
}

/// Test MCP status with typed response.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_mcp_status_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Get MCP status - returns typed McpStatus
    let status = client.mcp().status().await.expect("Failed to get MCP status");

    // Verify typed fields
    println!("MCP servers: {:?}", status.servers.len());
    for server in &status.servers {
        assert!(!server.name.is_empty(), "MCP server should have name");
        println!("  Server: {} - {:?}", server.name, server.status);
    }
}

/// Test LSP status with typed response.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_lsp_status_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Get LSP status - returns typed LspStatus
    let status = client.misc().lsp().await.expect("Failed to get LSP status");

    // LspStatus.servers is still Value, but we verify it deserializes
    println!("LSP servers: {:?}", status.servers);
}

/// Test formatter status with typed response.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_formatter_status_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Get formatter status - returns typed FormatterStatus
    let status = client
        .misc()
        .formatter()
        .await
        .expect("Failed to get formatter status");

    println!("Formatter enabled: {:?}", status.enabled);
}

/// Test OpenAPI doc with typed response.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_openapi_doc_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Get OpenAPI doc - returns typed OpenApiDoc
    let doc = client.misc().doc().await.expect("Failed to get OpenAPI doc");

    // Verify it's a valid OpenAPI document
    assert!(doc.spec.is_object(), "Doc should be a JSON object");
    assert!(
        doc.spec.get("openapi").is_some() || doc.spec.get("swagger").is_some(),
        "Should be an OpenAPI/Swagger document"
    );
}

/// Test find endpoints with typed responses.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_find_endpoints_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Find text - returns typed FindResponse
    let text_results = client
        .find()
        .text("fn main")
        .await
        .expect("Failed to search text");

    println!("Text search results: {:?}", text_results.results.is_some());

    // Find files - returns typed FindResponse
    let file_results = client
        .find()
        .files("*.rs")
        .await
        .expect("Failed to search files");

    println!("File search results: {:?}", file_results.results.is_some());

    // Find symbols - returns typed FindResponse
    let symbol_results = client
        .find()
        .symbols("main")
        .await
        .expect("Failed to search symbols");

    println!(
        "Symbol search results: {:?}",
        symbol_results.results.is_some()
    );
}

/// Test message list with typed Part deserialization.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_message_parts_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Create session
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    // Send a prompt
    let _ = client
        .messages()
        .prompt(
            &session.id,
            &PromptRequest {
                parts: vec![PromptPart::Text {
                    text: "Hello".to_string(),
                    synthetic: None,
                    ignored: None,
                    metadata: None,
                }],
                message_id: None,
                model: None,
                agent: None,
                no_reply: Some(true),
                system: None,
                variant: None,
            },
        )
        .await;

    // List messages - should have typed Parts
    let messages = client
        .messages()
        .list(&session.id)
        .await
        .expect("Failed to list messages");

    for message in &messages {
        println!("Message {} has {} parts", message.id, message.parts.len());
        for part in &message.parts {
            // Parts should deserialize to typed enum variants
            match part {
                opencode_rs::types::Part::Text { text, .. } => {
                    println!("  Text part: {}...", &text[..text.len().min(50)]);
                }
                opencode_rs::types::Part::Tool { tool, state, .. } => {
                    println!("  Tool part: {} - state: {:?}", tool, state.as_ref().map(|s| s.status()));
                }
                _ => {
                    println!("  Other part type");
                }
            }
        }
    }

    // Clean up
    let _ = client.sessions().delete(&session.id).await;
}

/// Test session with permission ruleset.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_session_permission_ruleset() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Create session - permission should deserialize as Ruleset if present
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    // Get the session and check permission field
    let fetched = client
        .sessions()
        .get(&session.id)
        .await
        .expect("Failed to get session");

    // Permission may or may not be set
    if let Some(permission) = &fetched.permission {
        println!("Session has {} permission rules", permission.len());
        for rule in permission {
            println!(
                "  Rule: {} {} {:?}",
                rule.permission, rule.pattern, rule.action
            );
        }
    }

    // Clean up
    let _ = client.sessions().delete(&session.id).await;
}
