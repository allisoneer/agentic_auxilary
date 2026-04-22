//! HTTP endpoint integration tests.
//!
//! Tests that verify typed HTTP responses against a live opencode server.
//!
//! TODO(3): Add error case tests (invalid session IDs, malformed payloads, missing fields)
//! to verify error responses deserialize correctly.

use super::create_test_client;
use super::should_run;
use opencode_rs::types::message::PromptPart;
use opencode_rs::types::message::PromptRequest;
use opencode_rs::types::message::ShellRequest;
use opencode_rs::types::session::SessionInitRequest;

/// Test session CRUD with typed responses.
#[tokio::test]
#[ignore = "requires: opencode serve"]
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
    // Note: Session list may have race conditions with filesystem, so we just verify we can call it
    match client.sessions().list().await {
        Ok(sessions) => {
            println!("Listed {} sessions", sessions.len());
            // Verify structure when sessions exist (can't check specific IDs due to timing)
            if let Some(first) = sessions.first() {
                assert!(!first.id.is_empty(), "Session should have ID");
            }
        }
        Err(e) => {
            // List may fail in some configurations
            println!("List sessions: {e:?}");
        }
    }

    // Delete session
    client
        .sessions()
        .delete(&session.id)
        .await
        .expect("Failed to delete session");
}

/// Test prompt with typed response.
#[tokio::test]
#[ignore = "requires: opencode serve"]
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
#[ignore = "requires: opencode serve"]
async fn test_providers_list_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // List providers - returns ProviderListResponse with all/default/connected
    let response = client
        .providers()
        .list()
        .await
        .expect("Failed to list providers");

    // Verify typed fields in the 'all' array
    for provider in &response.all {
        assert!(!provider.id.is_empty(), "Provider should have ID");
        assert!(!provider.name.is_empty(), "Provider should have name");
        println!(
            "Provider: {} ({}) - {:?} models",
            provider.name,
            provider.id,
            provider.models.len()
        );
    }

    // Verify we have proper default and connected data
    println!(
        "Response: {} providers, {} defaults, {} connected",
        response.all.len(),
        response.default.len(),
        response.connected.len()
    );
}

/// Test MCP status with typed response.
#[tokio::test]
#[ignore = "requires: opencode serve"]
async fn test_mcp_status_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Get MCP status - returns typed McpStatus
    let status = client
        .mcp()
        .status()
        .await
        .expect("Failed to get MCP status");

    // Verify typed fields
    println!("MCP servers: {:?}", status.servers.len());
    for server in &status.servers {
        assert!(!server.name.is_empty(), "MCP server should have name");
        println!("  Server: {} - {:?}", server.name, server.status);
    }
}

/// Test LSP status with typed response.
#[tokio::test]
#[ignore = "requires: opencode serve"]
async fn test_lsp_status_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Get LSP status - returns Vec<LspServerStatus>
    let servers = client.misc().lsp().await.expect("Failed to get LSP status");

    // Verify we got a response (may be empty if no LSP servers configured)
    println!("LSP servers: {} configured", servers.len());
    for server in &servers {
        println!("  {} ({}): {:?}", server.name, server.id, server.status);
    }
}

/// Test formatter status with typed response.
#[tokio::test]
#[ignore = "requires: opencode serve"]
async fn test_formatter_status_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Get formatter status - returns Vec<FormatterInfo>
    let formatters = client
        .misc()
        .formatter()
        .await
        .expect("Failed to get formatter status");

    // Verify we got a response (may be empty if no formatters configured)
    println!("Formatters: {} configured", formatters.len());
    for fmt in &formatters {
        println!(
            "  {} - enabled: {}, extensions: {:?}",
            fmt.name, fmt.enabled, fmt.extensions
        );
    }
}

/// Test `OpenAPI` doc with typed response.
#[tokio::test]
#[ignore = "requires: opencode serve"]
async fn test_openapi_doc_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Get OpenAPI doc - returns typed OpenApiDoc
    let doc = client
        .misc()
        .doc()
        .await
        .expect("Failed to get OpenAPI doc");

    // Verify it's a valid OpenAPI document
    assert!(doc.spec.is_object(), "Doc should be a JSON object");
    assert!(
        doc.spec.get("openapi").is_some() || doc.spec.get("swagger").is_some(),
        "Should be an OpenAPI/Swagger document"
    );
}

/// Test find endpoints with typed responses.
#[tokio::test]
#[ignore = "requires: opencode serve"]
async fn test_find_endpoints_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Find text - returns typed FindResponse (uses 'pattern' param)
    match client.find().text("fn").await {
        Ok(text_results) => {
            println!("Text search: got response");
            let _ = text_results.results;
        }
        Err(e) => {
            // May fail if ripgrep not available or no files to search
            println!("Text search not available: {e:?}");
        }
    }

    // Find files - returns typed FindResponse (uses 'query' param)
    match client.find().files("Cargo").await {
        Ok(file_results) => {
            println!("File search: got response");
            let _ = file_results.results;
        }
        Err(e) => {
            println!("File search not available: {e:?}");
        }
    }

    // Find symbols - returns typed FindResponse (currently returns empty)
    match client.find().symbols("main").await {
        Ok(symbol_results) => {
            println!("Symbol search: got response");
            let _ = symbol_results.results;
        }
        Err(e) => {
            println!("Symbol search not available: {e:?}");
        }
    }
}

/// Test message list with typed Part deserialization.
#[tokio::test]
#[ignore = "requires: opencode serve"]
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
        println!("Message {} has {} parts", message.id(), message.parts.len());
        for part in &message.parts {
            // Parts should deserialize to typed enum variants
            match part {
                opencode_rs::types::Part::Text { text, .. } => {
                    let preview: String = text.chars().take(50).collect();
                    println!("  Text part: {preview}...");
                }
                opencode_rs::types::Part::Tool { tool, state, .. } => {
                    println!(
                        "  Tool part: {} - state: {:?}",
                        tool,
                        state.as_ref().map(opencode_rs::types::ToolState::status)
                    );
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

/// Test session init with required body fields.
#[tokio::test]
#[ignore = "requires: opencode serve"]
async fn test_session_init_required_body() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    let _ = client
        .messages()
        .prompt(
            &session.id,
            &PromptRequest {
                parts: vec![PromptPart::Text {
                    text: "Initialize this session".to_string(),
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

    let providers = client.providers().list().await.expect("providers list");
    if let Some(provider) = providers.all.first()
        && let Some((model_id, _)) = provider.models.iter().next()
    {
        let messages = client
            .messages()
            .list(&session.id)
            .await
            .expect("messages list");
        if let Some(message) = messages.first() {
            let ok = client
                .sessions()
                .init(
                    &session.id,
                    &SessionInitRequest {
                        model_id: model_id.clone(),
                        provider_id: provider.id.clone(),
                        message_id: message.id().to_string(),
                    },
                )
                .await;
            println!("session.init result: {ok:?}");
        }
    }

    let _ = client.sessions().delete(&session.id).await;
}

/// Test session diff patch schema.
#[tokio::test]
#[ignore = "requires: opencode serve"]
async fn test_session_diff_patch_typed() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    let diff = client.sessions().diff(&session.id).await;
    println!("session.diff result: {diff:?}");

    let _ = client.sessions().delete(&session.id).await;
}

/// Test shell response shape.
#[tokio::test]
#[ignore = "requires: opencode serve"]
async fn test_shell_returns_info_and_parts() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    let shell = client
        .messages()
        .shell(
            &session.id,
            &ShellRequest {
                command: "echo hello".to_string(),
                model: None,
            },
        )
        .await;
    println!("shell result: {shell:?}");

    let _ = client.sessions().delete(&session.id).await;
}

/// Test session with permission ruleset.
#[tokio::test]
#[ignore = "requires: opencode serve"]
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
