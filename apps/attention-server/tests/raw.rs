#![expect(clippy::expect_used, reason = "integration test assertions")]

#[expect(dead_code, reason = "shared support has SDK-only fixtures")]
mod support;

use attention_kernel as k;
use attention_protocol as p;
use attention_server::ServerConfig;
use futures_util::SinkExt;
use futures_util::StreamExt;
use serde_json::Value;
use serde_json::json;
use std::sync::Arc;
use support::Barrier;
use support::ScriptedService;
use support::TestServer;
use support::WAIT;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

async fn connect(
    server: &TestServer,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    tokio_tungstenite::connect_async(server.url())
        .await
        .expect("websocket connect")
        .0
}

fn hello(id: &str, subscription: p::SubscriptionRequest) -> Message {
    let request = p::RpcRequest {
        jsonrpc: p::JsonRpcVersion,
        id: p::RequestId(id.into()),
        method: p::RPC_HELLO_METHOD.into(),
        params: Some(p::HelloRequest {
            protocol_version: p::PROTOCOL_V1,
            subscription,
            client: None,
        }),
    };
    Message::Text(
        serde_json::to_string(&request)
            .expect("serialize hello")
            .into(),
    )
}

async fn text_json(
    ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
) -> Value {
    let message = tokio::time::timeout(WAIT, ws.next())
        .await
        .expect("response timeout")
        .expect("response frame")
        .expect("response");
    serde_json::from_slice(&message.into_data()).expect("JSON response")
}

async fn hello_none(
    ws: &mut (
             impl SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error>
             + StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
             + Unpin
         ),
) {
    ws.send(hello("hello", p::SubscriptionRequest::None))
        .await
        .expect("send hello");
    assert_eq!(text_json(ws).await.get("id"), Some(&json!("hello")));
}

#[tokio::test]
async fn origin_absent_allowed_and_denied_matrix() {
    let mut config = ServerConfig::default();
    config
        .allowed_origins
        .insert("https://allowed.example".into());
    let server = TestServer::start(config, Arc::new(ScriptedService::empty(1))).await;

    let absent = tokio_tungstenite::connect_async(server.url()).await;
    assert!(absent.is_ok());
    drop(absent);

    for (origin, expected) in [
        ("https://allowed.example", true),
        ("https://denied.example", false),
    ] {
        let mut request = server.url().into_client_request().expect("request");
        request
            .headers_mut()
            .insert("Origin", HeaderValue::from_str(origin).expect("origin"));
        let result = tokio_tungstenite::connect_async(request).await;
        assert_eq!(result.is_ok(), expected, "origin {origin}");
        if let Err(tokio_tungstenite::tungstenite::Error::Http(response)) = result {
            assert_eq!(response.status(), 403);
        }
    }
    server.shutdown().await;
}

#[tokio::test]
async fn connection_cap_rejects_and_recovers() {
    let config = ServerConfig {
        max_connections: 1,
        ..ServerConfig::default()
    };
    let server = TestServer::start(config, Arc::new(ScriptedService::empty(1))).await;
    let first = connect(&server).await;
    let second = tokio_tungstenite::connect_async(server.url())
        .await
        .expect_err("cap must reject");
    match second {
        tokio_tungstenite::tungstenite::Error::Http(response) => assert_eq!(response.status(), 503),
        other => panic!("unexpected error: {other}"),
    }
    drop(first);
    let recovered = tokio::time::timeout(WAIT, async {
        loop {
            if let Ok(connection) = tokio_tungstenite::connect_async(server.url()).await {
                break connection;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("connection slot release timeout");
    drop(recovered);
    server.shutdown().await;
}

#[tokio::test]
async fn silent_peer_timeout_releases_connection_slot() {
    let config = ServerConfig {
        max_connections: 1,
        hello_frame_timeout: std::time::Duration::from_millis(50),
        ..ServerConfig::default()
    };
    let server = TestServer::start(config, Arc::new(ScriptedService::empty(1))).await;
    let mut silent = connect(&server).await;

    let rejected = tokio_tungstenite::connect_async(server.url())
        .await
        .expect_err("silent peer must hold the sole slot initially");
    match rejected {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), 503);
        }
        other => panic!("unexpected error: {other}"),
    }

    let close = tokio::time::timeout(WAIT, silent.next())
        .await
        .expect("silent-peer close timeout")
        .expect("silent-peer close frame")
        .expect("silent-peer close");
    assert!(matches!(close, Message::Close(Some(frame)) if u16::from(frame.code) == 1002));

    let mut recovered = tokio::time::timeout(WAIT, async {
        loop {
            if let Ok((connection, _)) = tokio_tungstenite::connect_async(server.url()).await {
                break connection;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("timed-out peer did not release connection slot");
    hello_none(&mut recovered).await;
    drop(recovered);
    server.shutdown().await;
}

#[tokio::test]
async fn hello_and_envelope_error_matrix() {
    let server =
        TestServer::start(ServerConfig::default(), Arc::new(ScriptedService::empty(1))).await;
    let first_cases = [
        (Message::Text("not json".into()), p::PARSE_ERROR),
        (Message::Text(json!({"jsonrpc":"2.0","id":"x","method":"other","params":{}}).to_string().into()), p::HELLO_REQUIRED),
        (Message::Text(json!({"jsonrpc":"2.0","id":"x","method":"rpc.hello"}).to_string().into()), p::INVALID_REQUEST),
        (Message::Text(json!({"jsonrpc":"2.0","id":"x","method":"rpc.hello","params":{"protocol_version":99,"subscription":{"mode":"none"}}}).to_string().into()), p::UNSUPPORTED_PROTOCOL_VERSION),
    ];
    for (message, code) in first_cases {
        let mut ws = connect(&server).await;
        ws.send(message).await.expect("send invalid hello");
        assert_eq!(text_json(&mut ws).await["error"]["code"], json!(code));
    }

    let malformed = [
        ("not json", p::PARSE_ERROR, Value::Null),
        (
            r#"{"jsonrpc":"1.0","id":"bad-version","method":"x","params":{}}"#,
            p::INVALID_REQUEST,
            json!("bad-version"),
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"method":"x","params":{}}"#,
            p::INVALID_REQUEST,
            Value::Null,
        ),
        (
            r#"{"jsonrpc":"2.0","id":"missing-params","method":"x"}"#,
            p::INVALID_REQUEST,
            json!("missing-params"),
        ),
        (
            r#"{"jsonrpc":"2.0","id":"unknown","method":"unknown.method","params":{}}"#,
            p::METHOD_NOT_FOUND,
            json!("unknown"),
        ),
    ];
    let mut ws = connect(&server).await;
    hello_none(&mut ws).await;
    for (body, code, id) in malformed {
        ws.send(Message::Text(body.into()))
            .await
            .expect("send request");
        let response = text_json(&mut ws).await;
        assert_eq!(response["error"]["code"], json!(code), "body {body}");
        assert_eq!(response["id"], id, "body {body}");
    }
    server.shutdown().await;
}

#[tokio::test]
async fn binary_and_oversized_input_are_closed() {
    let config = ServerConfig {
        max_message_bytes: 512,
        ..ServerConfig::default()
    };
    let server = TestServer::start(config, Arc::new(ScriptedService::empty(1))).await;
    let mut binary = connect(&server).await;
    hello_none(&mut binary).await;
    binary
        .send(Message::Binary(vec![1, 2].into()))
        .await
        .expect("binary");
    let close = tokio::time::timeout(WAIT, binary.next())
        .await
        .expect("close timeout")
        .expect("close frame")
        .expect("close");
    assert!(matches!(close, Message::Close(Some(frame)) if u16::from(frame.code) == 1003));

    let mut oversized = connect(&server).await;
    hello_none(&mut oversized).await;
    oversized
        .send(Message::Text("x".repeat(513).into()))
        .await
        .expect("oversized");
    let result = tokio::time::timeout(WAIT, oversized.next())
        .await
        .expect("oversize close timeout")
        .expect("oversize result");
    assert!(
        result.is_err()
            || matches!(result, Ok(Message::Close(Some(frame))) if u16::from(frame.code) == 1009)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn max_in_flight_error_keeps_request_correlation() {
    let barrier = Arc::new(Barrier::default());
    let mut service = ScriptedService::empty(1);
    service.read_barrier = Some(Arc::clone(&barrier));
    let config = ServerConfig {
        max_in_flight: 1,
        ..ServerConfig::default()
    };
    let server = TestServer::start(config, Arc::new(service)).await;
    let mut ws = connect(&server).await;
    hello_none(&mut ws).await;
    let id = k::WorkItemId::new().to_string();
    let request = |request_id: &str| {
        json!({"jsonrpc":"2.0","id":request_id,"method":"attention.work_item.get","params":{"id":id}}).to_string()
    };
    ws.send(Message::Text(request("blocked").into()))
        .await
        .expect("blocked request");
    barrier.wait_entered().await;
    ws.send(Message::Text(request("overload").into()))
        .await
        .expect("overload request");
    let response = text_json(&mut ws).await;
    assert_eq!(response["id"], "overload");
    assert_eq!(response["error"]["code"], json!(p::INTERNAL_ERROR));
    barrier.release();
    server.shutdown().await;
}

#[tokio::test]
async fn graceful_shutdown_pre_hello_and_during_blocked_request() {
    let config = ServerConfig {
        shutdown_grace: std::time::Duration::from_millis(100),
        ..ServerConfig::default()
    };
    let server = TestServer::start(config.clone(), Arc::new(ScriptedService::empty(1))).await;
    let mut pre_hello = connect(&server).await;
    server.state.shutdown.cancel();
    let result = tokio::time::timeout(WAIT, pre_hello.next())
        .await
        .expect("pre-hello shutdown timeout");
    let frame = result
        .expect("pre-hello close frame")
        .expect("pre-hello close");
    assert!(matches!(frame, Message::Close(Some(close)) if u16::from(close.code) == 1001));
    server.shutdown().await;

    let barrier = Arc::new(Barrier::default());
    let mut service = ScriptedService::empty(1);
    service.read_barrier = Some(Arc::clone(&barrier));
    let server = TestServer::start(config, Arc::new(service)).await;
    let mut ws = connect(&server).await;
    hello_none(&mut ws).await;
    let id = k::WorkItemId::new().to_string();
    ws.send(Message::Text(json!({"jsonrpc":"2.0","id":"blocked","method":"attention.work_item.get","params":{"id":id}}).to_string().into())).await.expect("blocked request");
    barrier.wait_entered().await;
    server.state.shutdown.cancel();
    let close = tokio::time::timeout(WAIT, ws.next())
        .await
        .expect("shutdown close timeout")
        .expect("close frame")
        .expect("close");
    assert!(matches!(close, Message::Close(Some(frame)) if u16::from(frame.code) == 1001));
    server.shutdown().await;
}
