//! Standalone mock server tests for opencode_rs.
//!
//! These tests verify the SDK works correctly against a wiremock server.

use opencode_rs::ClientBuilder;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Test that the client can get health status from the server.
#[tokio::test]
async fn get_health_ok() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/global/health"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"healthy": true, "version": "0.0.3"})),
        )
        .mount(&server)
        .await;

    let client = ClientBuilder::new().base_url(server.uri()).build().unwrap();

    let health = client.misc().health().await.unwrap();
    assert!(health.healthy);
    assert_eq!(health.version, Some("0.0.3".to_string()));
}

/// Test 404 error handling with NamedError response.
#[tokio::test]
async fn session_not_found_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/session/nonexistent"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "name": "NotFound",
            "message": "Session not found",
            "data": {"id": "nonexistent"}
        })))
        .mount(&server)
        .await;

    let client = ClientBuilder::new().base_url(server.uri()).build().unwrap();

    let result = client.sessions().get("nonexistent").await;

    match result {
        Err(opencode_rs::OpencodeError::Http {
            status,
            name,
            message,
            ..
        }) => {
            assert_eq!(status, 404);
            assert_eq!(name, Some("NotFound".to_string()));
            assert_eq!(message, "Session not found");
        }
        _ => panic!("Expected Http NotFound error"),
    }
}

/// Test 400 validation error handling.
#[tokio::test]
async fn validation_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "name": "ValidationError",
            "message": "Invalid request body"
        })))
        .mount(&server)
        .await;

    let client = ClientBuilder::new().base_url(server.uri()).build().unwrap();

    let result = client
        .sessions()
        .create(&opencode_rs::types::CreateSessionRequest::default())
        .await;

    match result {
        Err(err) => {
            assert!(err.is_validation_error());
            assert_eq!(err.error_name(), Some("ValidationError"));
        }
        _ => panic!("Expected validation error"),
    }
}

/// Test 500 server error handling.
#[tokio::test]
async fn server_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "name": "InternalError",
            "message": "Something went wrong"
        })))
        .mount(&server)
        .await;

    let client = ClientBuilder::new().base_url(server.uri()).build().unwrap();

    let result = client.sessions().list().await;

    match result {
        Err(err) => {
            assert!(err.is_server_error());
        }
        _ => panic!("Expected server error"),
    }
}

/// Test creating and deleting a session.
#[tokio::test]
async fn session_lifecycle() {
    let server = MockServer::start().await;

    // Create session
    Mock::given(method("POST"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "test-session-123",
            "projectId": "proj1",
            "directory": "/path/to/project",
            "title": "Test Session",
            "version": "1.0",
            "time": {"created": 1234567890, "updated": 1234567890}
        })))
        .mount(&server)
        .await;

    // Delete session
    Mock::given(method("DELETE"))
        .and(path("/session/test-session-123"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = ClientBuilder::new().base_url(server.uri()).build().unwrap();

    // Create
    let session = client
        .sessions()
        .create(&opencode_rs::types::CreateSessionRequest::default())
        .await
        .unwrap();
    assert_eq!(session.id, "test-session-123");
    assert_eq!(session.title, "Test Session");

    // Delete
    client.sessions().delete(&session.id).await.unwrap();
}

/// Test x-opencode-directory header is sent.
#[tokio::test]
async fn directory_header_sent() {
    use wiremock::matchers::header;

    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/session"))
        .and(header("x-opencode-directory", "/my/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())
        .directory("/my/project")
        .build()
        .unwrap();

    let sessions = client.sessions().list().await.unwrap();
    assert!(sessions.is_empty());
}
