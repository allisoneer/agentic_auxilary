#![allow(clippy::expect_used, clippy::unwrap_used)]

use opencode_rs::ClientBuilder;
use opencode_rs::types::permission::PermissionReply;
use opencode_rs::types::v2::permission::PermissionReplyRequest;
use opencode_rs::types::v2::question::QuestionReply;
use opencode_rs::types::v2::session::CreateSessionRequest;
use opencode_rs::types::v2::session::Prompt;
use opencode_rs::types::v2::session::SessionDelivery;
use opencode_rs::types::v2::session::SessionListOrder;
use opencode_rs::types::v2::session::SessionListParams;
use opencode_rs::types::v2::session::SessionPromptRequest;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_json;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;

#[tokio::test]
async fn v2_health_and_location_use_expected_shapes() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "healthy": true
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/location"))
        .and(query_param("location[directory]", "/tmp/project"))
        .and(query_param("location[workspace]", "workspace-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "directory": "/tmp/project",
            "workspaceID": "workspace-1",
            "project": {"id": "prj_1", "directory": "/tmp/project"}
        })))
        .mount(&mock)
        .await;

    let client = ClientBuilder::new()
        .base_url(mock.uri())
        .directory("/tmp/project")
        .workspace("workspace-1")
        .build()
        .unwrap();

    let health = client.v2().health().get().await.unwrap();
    assert!(health.healthy);

    let location = client.v2().location().get().await.unwrap();
    assert_eq!(location.directory.as_deref(), Some("/tmp/project"));
    assert_eq!(location.workspace_id.as_deref(), Some("workspace-1"));
}

#[tokio::test]
async fn v2_session_and_message_groups_deserialize_envelopes() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/session"))
        .and(query_param("location[directory]", "/tmp/project"))
        .and(query_param("order", "desc"))
        .and(query_param("limit", "25"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {
                    "id": "ses_1",
                    "title": "Session One",
                    "projectID": "prj_1",
                    "location": {"directory": "/tmp/project"},
                    "time": {"created": 1, "updated": 2}
                }
            ],
            "cursor": {"next": "cursor-next"}
        })))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/session"))
        .and(body_json(serde_json::json!({"agent": "Plan"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "id": "ses_new",
                "title": "",
                "location": {"directory": "/tmp/project"}
            }
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/session/ses_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"id": "ses_1", "title": "Session One", "location": {"directory": "/tmp/project"}}
        })))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/session/ses_1/prompt"))
        .and(body_json(serde_json::json!({
            "prompt": {"text": "hello"},
            "delivery": "queue"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "admittedSeq": 5,
                "id": "msg_1",
                "sessionID": "ses_1",
                "prompt": {"text": "hello"},
                "delivery": "queue",
                "timeCreated": 123
            }
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/session/ses_1/message"))
        .and(query_param("cursor", "cursor-next"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{"type": "assistant", "id": "msg_1"}],
            "cursor": {"previous": "cursor-prev"}
        })))
        .mount(&mock)
        .await;

    let client = ClientBuilder::new()
        .base_url(mock.uri())
        .directory("/tmp/project")
        .build()
        .unwrap();

    let sessions = client
        .v2()
        .session()
        .list(&SessionListParams {
            limit: Some(25),
            order: Some(SessionListOrder::Desc),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(sessions.data[0].id, "ses_1");
    assert_eq!(
        sessions.cursor.and_then(|c| c.next).as_deref(),
        Some("cursor-next")
    );

    let created = client
        .v2()
        .session()
        .create(&CreateSessionRequest {
            agent: Some("Plan".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(created.data.id, "ses_new");

    let got = client.v2().session().get("ses_1").await.unwrap();
    assert_eq!(got.data.title, "Session One");

    let admitted = client
        .v2()
        .session()
        .prompt(
            "ses_1",
            &SessionPromptRequest {
                id: None,
                prompt: Prompt {
                    text: "hello".to_string(),
                    ..Default::default()
                },
                delivery: Some(SessionDelivery::Queue),
                resume: None,
                extra: serde_json::Value::Null,
            },
        )
        .await
        .unwrap();
    assert_eq!(admitted.data.session_id, "ses_1");

    let messages = client
        .v2()
        .message()
        .list("ses_1", Some("cursor-next"))
        .await
        .unwrap();
    assert_eq!(messages.data.len(), 1);
    assert_eq!(
        messages.cursor.and_then(|c| c.previous).as_deref(),
        Some("cursor-prev")
    );
}

#[tokio::test]
async fn v2_model_and_provider_groups_use_location_wrapped_responses() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/model"))
        .and(query_param("location[directory]", "/tmp/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": [{"id": "claude-sonnet", "providerID": "anthropic"}]
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/provider"))
        .and(query_param("location[directory]", "/tmp/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": [{"id": "anthropic", "name": "Anthropic"}]
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/provider/anthropic"))
        .and(query_param("location[directory]", "/tmp/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": {"id": "anthropic", "name": "Anthropic"}
        })))
        .mount(&mock)
        .await;

    let client = ClientBuilder::new()
        .base_url(mock.uri())
        .directory("/tmp/project")
        .build()
        .unwrap();

    let models = client.v2().model().list().await.unwrap();
    assert_eq!(models.location.directory.as_deref(), Some("/tmp/project"));
    assert_eq!(models.data.len(), 1);

    let providers = client.v2().provider().list().await.unwrap();
    assert_eq!(providers.data.len(), 1);

    let provider = client.v2().provider().get("anthropic").await.unwrap();
    assert_eq!(provider.data["id"], "anthropic");
}

#[tokio::test]
async fn v2_permission_and_question_groups_support_list_and_204_replies() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/permission/request"))
        .and(query_param("location[directory]", "/tmp/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": [{
                "id": "per_1",
                "sessionID": "ses_1",
                "action": "fs.write",
                "resources": ["/tmp/project/src/lib.rs"]
            }]
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/session/ses_1/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{
                "id": "per_1",
                "sessionID": "ses_1",
                "action": "fs.write",
                "resources": ["/tmp/project/src/lib.rs"]
            }]
        })))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/session/ses_1/permission/per_1/reply"))
        .and(body_json(
            serde_json::json!({"reply": "always", "message": "ok"}),
        ))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/question/request"))
        .and(query_param("location[directory]", "/tmp/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": [{
                "id": "que_1",
                "sessionID": "ses_1",
                "questions": [{"question": "Continue?"}]
            }]
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/session/ses_1/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{
                "id": "que_1",
                "sessionID": "ses_1",
                "questions": [{"question": "Continue?"}]
            }]
        })))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/session/ses_1/question/que_1/reply"))
        .and(body_json(serde_json::json!({"answers": [["Yes"]]})))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/session/ses_1/question/que_1/reject"))
        .and(body_json(serde_json::json!({})))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    let client = ClientBuilder::new()
        .base_url(mock.uri())
        .directory("/tmp/project")
        .build()
        .unwrap();

    let permissions = client.v2().permission().list_requests().await.unwrap();
    assert_eq!(permissions.data.len(), 1);
    assert_eq!(
        permissions.location.directory.as_deref(),
        Some("/tmp/project")
    );

    let session_permissions = client
        .v2()
        .permission()
        .list_session_requests("ses_1")
        .await
        .unwrap();
    assert_eq!(session_permissions.data[0].id, "per_1");

    client
        .v2()
        .permission()
        .reply(
            "ses_1",
            "per_1",
            &PermissionReplyRequest {
                reply: PermissionReply::Always,
                message: Some("ok".to_string()),
            },
        )
        .await
        .unwrap();

    let questions = client.v2().question().list_requests().await.unwrap();
    assert_eq!(questions.data.len(), 1);

    let session_questions = client
        .v2()
        .question()
        .list_session_requests("ses_1")
        .await
        .unwrap();
    assert_eq!(session_questions.data[0].id, "que_1");

    client
        .v2()
        .question()
        .reply(
            "ses_1",
            "que_1",
            &QuestionReply {
                answers: vec![vec!["Yes".to_string()]],
            },
        )
        .await
        .unwrap();

    client
        .v2()
        .question()
        .reject("ses_1", "que_1")
        .await
        .unwrap();
}

#[tokio::test]
async fn v2_optional_connector_fs_and_reference_groups_cover_json_only_endpoints() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/connector"))
        .and(query_param("location[directory]", "/tmp/project"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": [{"id": "con_1", "name": "GitHub"}]
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/connector/con_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": {"id": "con_1", "name": "GitHub"}
        })))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/connector/con_1/connect/key"))
        .and(body_json(
            serde_json::json!({"methodID": "key", "key": "secret", "inputs": {}}),
        ))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/connector/con_1/connect/oauth"))
        .and(body_json(
            serde_json::json!({"methodID": "oauth", "inputs": {}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": {"attemptID": "att_1", "url": "https://example.com/oauth"}
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/connector/oauth/att_1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": {"status": "pending"}
        })))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/connector/oauth/att_1/complete"))
        .and(body_json(serde_json::json!({"code": "abc123"})))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    Mock::given(method("DELETE"))
        .and(path("/api/connector/oauth/att_1"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/fs/list"))
        .and(query_param("path", "src"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": [{"path": "src/lib.rs", "type": "file", "mime": "text/x-rust"}]
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/fs/find"))
        .and(query_param("query", "lib"))
        .and(query_param("type", "file"))
        .and(query_param("limit", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": [{"path": "src/lib.rs", "type": "file", "mime": "text/x-rust"}]
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/reference"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "location": {"directory": "/tmp/project"},
            "data": [{"name": "docs", "path": "/tmp/project/docs"}]
        })))
        .mount(&mock)
        .await;

    let client = ClientBuilder::new()
        .base_url(mock.uri())
        .directory("/tmp/project")
        .build()
        .unwrap();

    let connectors = client.v2().connector().list().await.unwrap();
    assert_eq!(connectors.data.len(), 1);

    let connector = client.v2().connector().get("con_1").await.unwrap();
    assert_eq!(connector.data.unwrap()["id"], "con_1");

    client
        .v2()
        .connector()
        .connect_key(
            "con_1",
            &serde_json::json!({"methodID": "key", "key": "secret", "inputs": {}}),
        )
        .await
        .unwrap();

    let attempt = client
        .v2()
        .connector()
        .connect_oauth_begin(
            "con_1",
            &serde_json::json!({"methodID": "oauth", "inputs": {}}),
        )
        .await
        .unwrap();
    assert_eq!(attempt.data["attemptID"], "att_1");

    let status = client.v2().connector().oauth_status("att_1").await.unwrap();
    assert_eq!(status.data["status"], "pending");

    client
        .v2()
        .connector()
        .oauth_complete("att_1", &serde_json::json!({"code": "abc123"}))
        .await
        .unwrap();
    client.v2().connector().oauth_cancel("att_1").await.unwrap();

    let entries = client.v2().fs().list(Some("src")).await.unwrap();
    assert_eq!(entries.data.len(), 1);

    let found = client
        .v2()
        .fs()
        .find("lib", Some("file"), Some(10))
        .await
        .unwrap();
    assert_eq!(found.data.len(), 1);

    let references = client.v2().reference().list().await.unwrap();
    assert_eq!(references.data.len(), 1);
}
