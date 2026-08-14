#![expect(clippy::expect_used, reason = "test setup and assertions")]

use attention_client::Client;
use attention_client::ClientConfig;
use attention_client::ClientError;
use attention_client::ConnectionStatus;
use attention_protocol as protocol;
use futures_util::SinkExt;
use futures_util::StreamExt;
use protocol::AttentionHelloResult;
use protocol::AttentionSnapshot;
use protocol::BootId;
use protocol::Cursor;
use protocol::EmptyParams;
use protocol::HelloLimits;
use protocol::JsonRpcVersion;
use protocol::PROTOCOL_V1;
use protocol::RequestId;
use protocol::ResponseId;
use protocol::RpcRequest;
use protocol::RpcResponse;
use protocol::RpcResponsePayload;
use protocol::ServerId;
use protocol::StreamId;
use protocol::SubscriptionRequest;
use protocol::SubscriptionResult;
use serde_json::Value;
use serde_json::json;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

type Peer = WebSocketStream<TcpStream>;

fn hello_result(
    subscription_result: SubscriptionResult<AttentionSnapshot>,
) -> AttentionHelloResult {
    AttentionHelloResult {
        protocol_version: PROTOCOL_V1,
        server_id: ServerId("server-test".into()),
        boot_id: BootId("boot-test".into()),
        stream_id: StreamId("stream-test".into()),
        subscription_result,
        limits: HelloLimits {
            max_message_bytes: 1_000_000,
            max_in_flight: 8,
        },
    }
}

fn snapshot(cursor: &str) -> SubscriptionResult<AttentionSnapshot> {
    SubscriptionResult::Snapshot {
        state: AttentionSnapshot {
            work_items: vec![],
            attention_signals: vec![],
            reminders: vec![],
            inbox: vec![],
        },
        after_cursor: Cursor(cursor.into()),
    }
}

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("ws://{}", listener.local_addr().expect("address"));
    (listener, url)
}

async fn accept_hello(listener: &TcpListener) -> (Peer, RpcRequest<Value>) {
    let (tcp, _) = listener.accept().await.expect("accept");
    let mut ws = accept_async(tcp).await.expect("websocket");
    let frame = ws.next().await.expect("hello frame").expect("hello read");
    let request = serde_json::from_slice(&frame.into_data()).expect("hello JSON");
    (ws, request)
}

async fn reply<T: serde::Serialize>(ws: &mut Peer, id: RequestId, payload: RpcResponsePayload<T>) {
    let response = RpcResponse {
        jsonrpc: JsonRpcVersion,
        id: ResponseId::Request(id),
        payload,
    };
    ws.send(Message::Text(
        serde_json::to_string(&response).expect("JSON").into(),
    ))
    .await
    .expect("send");
}

async fn hello_ok(ws: &mut Peer, request: RpcRequest<Value>, result: AttentionHelloResult) {
    reply(ws, request.id, RpcResponsePayload::Success(result)).await;
}

fn config(url: String) -> ClientConfig {
    let mut config = ClientConfig::new(url);
    config.reconnect_min = Duration::from_millis(5);
    config.reconnect_max = Duration::from_millis(5);
    config.heartbeat_interval = Duration::from_secs(10);
    config.request_timeout = Duration::from_millis(300);
    config
}

fn assert_configuration_error(config: ClientConfig) {
    assert!(matches!(
        Client::connect(config),
        Err(ClientError::Configuration(_))
    ));
}

#[test]
fn zero_request_timeout_is_rejected_before_spawn() {
    let mut config = ClientConfig::new("ws://127.0.0.1:9");
    config.request_timeout = Duration::ZERO;
    assert_configuration_error(config);
}

#[test]
fn zero_heartbeat_interval_is_rejected_before_spawn() {
    let mut config = ClientConfig::new("ws://127.0.0.1:9");
    config.heartbeat_interval = Duration::ZERO;
    assert_configuration_error(config);
}

#[test]
fn zero_heartbeat_timeout_is_rejected_before_spawn() {
    let mut config = ClientConfig::new("ws://127.0.0.1:9");
    config.heartbeat_timeout = Duration::ZERO;
    assert_configuration_error(config);
}

async fn next_text(ws: &mut Peer) -> RpcRequest<Value> {
    loop {
        match ws.next().await.expect("frame").expect("read") {
            Message::Text(text) => return serde_json::from_str(&text).expect("request"),
            Message::Ping(data) => ws.send(Message::Pong(data)).await.expect("pong"),
            other => panic!("unexpected frame {other:?}"),
        }
    }
}

#[tokio::test]
async fn snapshot_requires_ack_before_resume_and_clean_close() {
    let (listener, url) = listener().await;
    let peer = tokio::spawn(async move {
        let (mut first, hello) = accept_hello(&listener).await;
        hello_ok(&mut first, hello, hello_result(snapshot("cursor-0"))).await;
        first.close(None).await.expect("close first");
        let (mut second, hello) = accept_hello(&listener).await;
        let params: protocol::HelloRequest =
            serde_json::from_value(hello.params.clone().expect("params")).expect("params type");
        assert_eq!(params.subscription, SubscriptionRequest::Snapshot);
        hello_ok(&mut second, hello, hello_result(snapshot("cursor-0"))).await;
        while let Some(Ok(message)) = second.next().await {
            if matches!(message, Message::Close(_)) {
                break;
            }
        }
    });
    let (client, mut sub) = Client::connect(config(url)).expect("client");
    let first = sub.snapshots.recv().await.expect("first snapshot");
    let second = sub.snapshots.recv().await.expect("second snapshot");
    client
        .acknowledge_snapshot(second.after_cursor)
        .await
        .expect("ack");
    assert_eq!(first.after_cursor, Cursor("cursor-0".into()));
    client.close().await.expect("close");
    assert_eq!(*client.status().borrow(), ConnectionStatus::Closed);
    peer.await.expect("peer");
}

#[tokio::test]
async fn mutation_queued_before_connection_is_sent_without_ambiguity() {
    let reserved = TcpListener::bind("127.0.0.1:0").await.expect("reserve");
    let address = reserved.local_addr().expect("address");
    drop(reserved);
    let mut cfg = config(format!("ws://{address}"));
    cfg.request_timeout = Duration::from_secs(1);
    let (client, _) = Client::connect(cfg).expect("client");
    let call = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .delivery_claim(protocol::DeliveryClaimParams {
                    eligible_at: protocol::WireTimestamp::parse("2026-01-01T00:00:00.000000Z")
                        .expect("time"),
                    lease_expires_at: protocol::WireTimestamp::parse("2026-01-01T00:01:00.000000Z")
                        .expect("time"),
                    limit: 1,
                })
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let listener = TcpListener::bind(address).await.expect("bind delayed peer");
    let (mut ws, hello) = accept_hello(&listener).await;
    hello_ok(&mut ws, hello, hello_result(SubscriptionResult::None)).await;
    let request = next_text(&mut ws).await;
    reply(
        &mut ws,
        request.id,
        RpcResponsePayload::Success(json!({ "claims": [] })),
    )
    .await;
    call.await.expect("call task").expect("unambiguous result");
    client.close().await.expect("close");
}

async fn assert_mutation_disconnect_is_ambiguous() {
    let (listener, url) = listener().await;
    let peer = tokio::spawn(async move {
        let (mut first, hello) = accept_hello(&listener).await;
        hello_ok(&mut first, hello, hello_result(SubscriptionResult::None)).await;
        let request = next_text(&mut first).await;
        assert_eq!(request.method, "attention.delivery.claim");
        first.close(None).await.expect("close");
        let (mut second, hello) = accept_hello(&listener).await;
        hello_ok(&mut second, hello, hello_result(SubscriptionResult::None)).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(80), next_text(&mut second))
                .await
                .is_err()
        );
    });
    let (client, _) = Client::connect(config(url)).expect("client");
    let params = protocol::DeliveryClaimParams {
        eligible_at: protocol::WireTimestamp::parse("2026-01-01T00:00:00.000000Z").expect("time"),
        lease_expires_at: protocol::WireTimestamp::parse("2026-01-01T00:01:00.000000Z")
            .expect("time"),
        limit: 1,
    };
    assert!(matches!(
        client.delivery_claim(params).await,
        Err(ClientError::AmbiguousMutation)
    ));
    peer.await.expect("peer");
    client.close().await.expect("close");
}

#[tokio::test]
async fn mutation_disconnect_after_send_is_ambiguous_and_never_replayed() {
    assert_mutation_disconnect_is_ambiguous().await;
}

#[tokio::test]
async fn committed_mutation_without_response_is_ambiguous() {
    // Wire behavior is intentionally identical to after-send disconnect: the client cannot know
    // whether the peer committed, and therefore must report ambiguity in both cases.
    assert_mutation_disconnect_is_ambiguous().await;
}

#[tokio::test]
async fn read_replays_on_reconnect_with_original_deadline() {
    let (listener, url) = listener().await;
    let peer =
        tokio::spawn(async move {
            let (mut first, hello) = accept_hello(&listener).await;
            hello_ok(&mut first, hello, hello_result(SubscriptionResult::None)).await;
            let first_request = next_text(&mut first).await;
            first.close(None).await.expect("close");
            let (mut second, hello) = accept_hello(&listener).await;
            hello_ok(&mut second, hello, hello_result(SubscriptionResult::None)).await;
            let replay = next_text(&mut second).await;
            assert_eq!(replay.id, first_request.id);
            reply(&mut second, replay.id, RpcResponsePayload::Success(json!({
            "state": {"work_items": [], "attention_signals": [], "reminders": [], "inbox": []},
            "after_cursor": "cursor-1"
        }))).await;
        });
    let (client, _) = Client::connect(config(url)).expect("client");
    client
        .snapshot_get(EmptyParams {})
        .await
        .expect("replayed read");
    peer.await.expect("peer");
    client.close().await.expect("close");
}

#[tokio::test]
async fn configured_resume_gap_falls_back_to_snapshot_and_reports_peer_issue() {
    let (listener, url) = listener().await;
    let peer = tokio::spawn(async move {
        let (mut first, hello) = accept_hello(&listener).await;
        let error = protocol::RpcError {
            code: protocol::CURSOR_GAP,
            message: "gap".into(),
            data: None,
        };
        reply(
            &mut first,
            hello.id,
            RpcResponsePayload::<Value>::Error(error),
        )
        .await;
        let (mut second, hello) = accept_hello(&listener).await;
        let params: protocol::HelloRequest =
            serde_json::from_value(hello.params.clone().expect("params")).expect("params type");
        assert_eq!(params.subscription, SubscriptionRequest::Snapshot);
        hello_ok(&mut second, hello, hello_result(snapshot("fresh"))).await;
    });
    let mut cfg = config(url);
    cfg.subscription = SubscriptionRequest::Resume {
        server_id: ServerId("old-server".into()),
        stream_id: StreamId("old-stream".into()),
        after_cursor: Cursor("old".into()),
    };
    let (client, mut sub) = Client::connect(cfg).expect("client");
    assert!(matches!(
        sub.issues.recv().await.expect("issue").error,
        ClientError::Peer(_)
    ));
    assert_eq!(
        sub.snapshots.recv().await.expect("snapshot").after_cursor,
        Cursor("fresh".into())
    );
    peer.await.expect("peer");
    client.close().await.expect("close");
}

#[tokio::test]
async fn forced_snapshot_gap_uses_reconnect_backoff() {
    let (listener, url) = listener().await;
    let peer = tokio::spawn(async move {
        let gap = || protocol::RpcError {
            code: protocol::CURSOR_GAP,
            message: "gap".into(),
            data: None,
        };
        let (mut first, hello) = accept_hello(&listener).await;
        reply(
            &mut first,
            hello.id,
            RpcResponsePayload::<Value>::Error(gap()),
        )
        .await;

        let (mut second, hello) =
            tokio::time::timeout(Duration::from_millis(100), accept_hello(&listener))
                .await
                .expect("immediate snapshot fallback");
        let params: protocol::HelloRequest =
            serde_json::from_value(hello.params.clone().expect("params")).expect("params type");
        assert_eq!(params.subscription, SubscriptionRequest::Snapshot);
        let retry_started = tokio::time::Instant::now();
        reply(
            &mut second,
            hello.id,
            RpcResponsePayload::<Value>::Error(gap()),
        )
        .await;

        let (mut third, hello) =
            tokio::time::timeout(Duration::from_secs(1), accept_hello(&listener))
                .await
                .expect("backed-off retry");
        assert!(retry_started.elapsed() >= Duration::from_millis(250));
        let params: protocol::HelloRequest =
            serde_json::from_value(hello.params.clone().expect("params")).expect("params type");
        assert_eq!(params.subscription, SubscriptionRequest::Snapshot);
        hello_ok(&mut third, hello, hello_result(snapshot("fresh"))).await;
    });
    let mut cfg = config(url);
    cfg.subscription = SubscriptionRequest::Resume {
        server_id: ServerId("old-server".into()),
        stream_id: StreamId("old-stream".into()),
        after_cursor: Cursor("old".into()),
    };
    cfg.reconnect_min = Duration::from_millis(300);
    cfg.reconnect_max = Duration::from_millis(300);
    let (client, mut sub) = Client::connect(cfg).expect("client");
    assert_eq!(
        sub.snapshots.recv().await.expect("snapshot").after_cursor,
        Cursor("fresh".into())
    );
    peer.await.expect("peer");
    client.close().await.expect("close");
}

#[tokio::test]
async fn failed_hello_uses_reconnect_backoff() {
    let (listener, url) = listener().await;
    let peer = tokio::spawn(async move {
        let (mut first, hello) = accept_hello(&listener).await;
        let mut result = hello_result(SubscriptionResult::None);
        result.protocol_version = protocol::ProtocolVersion(99);
        let retry_started = tokio::time::Instant::now();
        hello_ok(&mut first, hello, result).await;

        let (mut second, hello) =
            tokio::time::timeout(Duration::from_secs(1), accept_hello(&listener))
                .await
                .expect("backed-off retry");
        assert!(retry_started.elapsed() >= Duration::from_millis(100));
        hello_ok(&mut second, hello, hello_result(SubscriptionResult::None)).await;
    });
    let mut cfg = config(url);
    cfg.reconnect_min = Duration::from_millis(150);
    cfg.reconnect_max = Duration::from_millis(150);
    let (client, mut sub) = Client::connect(cfg).expect("client");
    assert!(matches!(
        sub.issues.recv().await.expect("local issue").error,
        ClientError::LocalProtocol(_)
    ));
    peer.await.expect("peer");
    client.close().await.expect("close");
}

#[tokio::test]
async fn event_overflow_disconnects_and_resumes_after_acknowledged_snapshot() {
    let (listener, url) = listener().await;
    let peer = tokio::spawn(async move {
        let (mut first, hello) = accept_hello(&listener).await;
        hello_ok(&mut first, hello, hello_result(snapshot("base"))).await;
        for n in 1..=2 {
            let notification = json!({
                "jsonrpc":"2.0", "method":"attention.change", "params":{"event":{
                    "id":format!("event-{n}"), "cursor":format!("cursor-{n}"),
                    "occurred_at":"2026-01-01T00:00:00.000000Z", "kind":"work_item_created",
                    "affected":[], "inbox":{"upserts":[],"removals":[]}
                }}
            });
            first
                .send(Message::Text(notification.to_string().into()))
                .await
                .expect("event");
        }
        let (mut second, hello) = accept_hello(&listener).await;
        let params: protocol::HelloRequest =
            serde_json::from_value(hello.params.clone().expect("params")).expect("params type");
        assert!(
            matches!(params.subscription, SubscriptionRequest::Resume { after_cursor: Cursor(ref c), .. } if c == "base")
        );
        hello_ok(
            &mut second,
            hello,
            hello_result(SubscriptionResult::Resume {
                after_cursor: Cursor("base".into()),
            }),
        )
        .await;
    });
    let mut cfg = config(url);
    cfg.event_capacity = 1;
    let (client, mut sub) = Client::connect(cfg).expect("client");
    let snapshot = sub.snapshots.recv().await.expect("snapshot");
    client
        .acknowledge_snapshot(snapshot.after_cursor)
        .await
        .expect("ack");
    assert!(matches!(
        sub.issues.recv().await.expect("overflow").error,
        ClientError::Backpressure("event queue")
    ));
    peer.await.expect("peer");
    client.close().await.expect("close");
}

#[tokio::test]
async fn missing_pong_uses_independent_deadline() {
    let (listener, url) = listener().await;
    let peer = tokio::spawn(async move {
        let (mut ws, hello) = accept_hello(&listener).await;
        hello_ok(&mut ws, hello, hello_result(SubscriptionResult::None)).await;
        while let Some(Ok(message)) = ws.next().await {
            if matches!(message, Message::Ping(_)) {
                tokio::time::sleep(Duration::from_millis(100)).await;
                break;
            }
        }
    });
    let mut cfg = config(url);
    cfg.heartbeat_interval = Duration::from_millis(10);
    cfg.heartbeat_timeout = Duration::from_millis(20);
    let (client, mut sub) = Client::connect(cfg).expect("client");
    let issue = tokio::time::timeout(Duration::from_millis(80), sub.issues.recv())
        .await
        .expect("deadline")
        .expect("issue");
    assert!(
        matches!(issue.error, ClientError::Transport(ref text) if text.contains("pong timed out"))
    );
    peer.await.expect("peer");
    client.close().await.expect("close");
}

#[tokio::test]
async fn peer_local_transport_timeout_and_backpressure_are_distinct() {
    // Transport is observable even when the supervisor reconnects.
    let mut cfg = config("ws://127.0.0.1:9".into());
    cfg.command_capacity = 1;
    cfg.reconnect_min = Duration::from_secs(1);
    cfg.reconnect_max = Duration::from_secs(1);
    let (client, mut sub) = Client::connect(cfg).expect("client");
    assert!(matches!(
        sub.issues.recv().await.expect("transport").error,
        ClientError::Transport(_)
    ));
    let first = tokio::spawn({
        let client = client.clone();
        async move { client.snapshot_get(EmptyParams {}).await }
    });
    tokio::task::yield_now().await;
    let mut saw_backpressure = false;
    for _ in 0..8 {
        if matches!(
            client.snapshot_get(EmptyParams {}).await,
            Err(ClientError::Backpressure(_))
        ) {
            saw_backpressure = true;
            break;
        }
    }
    assert!(saw_backpressure);
    client.close().await.expect("close");
    assert!(first.await.expect("task").is_err());

    // Local negotiation errors remain distinguishable from peer RPC errors.
    let (local_listener, url) = listener().await;
    let peer = tokio::spawn(async move {
        let (mut ws, hello) = accept_hello(&local_listener).await;
        let mut result = hello_result(SubscriptionResult::None);
        result.protocol_version = protocol::ProtocolVersion(99);
        hello_ok(&mut ws, hello, result).await;
    });
    let (client, mut sub) = Client::connect(config(url)).expect("client");
    assert!(matches!(
        sub.issues.recv().await.expect("local").error,
        ClientError::LocalProtocol(_)
    ));
    peer.await.expect("peer");
    client.close().await.expect("close");

    // Correlated peer errors and request deadlines are delivered to the caller distinctly.
    let (listener, url) = listener().await;
    let peer = tokio::spawn(async move {
        let (mut ws, hello) = accept_hello(&listener).await;
        hello_ok(&mut ws, hello, hello_result(SubscriptionResult::None)).await;
        let request = next_text(&mut ws).await;
        reply(
            &mut ws,
            request.id,
            RpcResponsePayload::<Value>::Error(protocol::RpcError {
                code: protocol::RESOURCE_NOT_FOUND,
                message: "missing".into(),
                data: None,
            }),
        )
        .await;
        let _timed_out = next_text(&mut ws).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    let mut cfg = config(url);
    cfg.request_timeout = Duration::from_millis(30);
    let (client, _) = Client::connect(cfg).expect("client");
    assert!(matches!(
        client.snapshot_get(EmptyParams {}).await,
        Err(ClientError::Peer(_))
    ));
    assert!(matches!(
        client.snapshot_get(EmptyParams {}).await,
        Err(ClientError::Timeout)
    ));
    peer.await.expect("peer");
    client.close().await.expect("close");
}

#[tokio::test]
async fn generic_signal_acknowledge_is_mutation_and_never_replayed() {
    let (listener, url) = listener().await;
    let peer = tokio::spawn(async move {
        let (mut first, hello) = accept_hello(&listener).await;
        hello_ok(&mut first, hello, hello_result(SubscriptionResult::None)).await;
        let request = next_text(&mut first).await;
        assert_eq!(request.method, "attention.signal.acknowledge");
        first.close(None).await.expect("close first");

        let (mut second, hello) = accept_hello(&listener).await;
        hello_ok(&mut second, hello, hello_result(SubscriptionResult::None)).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(80), next_text(&mut second))
                .await
                .is_err()
        );
    });
    let (client, _) = Client::connect(config(url)).expect("client");
    let params = protocol::AcknowledgeAttentionSignalParams {
        id: protocol::AttentionSignalId("signal-1".into()),
        expected_revision: protocol::Revision::parse("1").expect("revision"),
        idempotency_key: protocol::MutationIdempotencyKey("mutation-1".into()),
    };
    assert!(matches!(
        client
            .call::<protocol::AttentionSignalAcknowledge>(params)
            .await,
        Err(ClientError::AmbiguousMutation)
    ));
    peer.await.expect("peer");
    client.close().await.expect("close");
}

#[tokio::test]
async fn unrelated_snapshot_ack_is_rejected_without_resume_corruption() {
    let (listener, url) = listener().await;
    let (close_first_tx, close_first_rx) = tokio::sync::oneshot::channel();
    let (second_ready_tx, second_ready_rx) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut first, hello) = accept_hello(&listener).await;
        hello_ok(&mut first, hello, hello_result(snapshot("snapshot-1"))).await;
        close_first_rx.await.expect("first close signal");
        first.close(None).await.expect("close first");

        let (mut second, hello) = accept_hello(&listener).await;
        let params: protocol::HelloRequest =
            serde_json::from_value(hello.params.clone().expect("params")).expect("params type");
        assert!(matches!(
            params.subscription,
            SubscriptionRequest::Resume {
                after_cursor: Cursor(ref cursor),
                ..
            } if cursor == "snapshot-1"
        ));
        hello_ok(
            &mut second,
            hello,
            hello_result(SubscriptionResult::Resume {
                after_cursor: Cursor("snapshot-1".into()),
            }),
        )
        .await;
        second_ready_tx.send(()).expect("second hello signal");
        while let Some(Ok(message)) = second.next().await {
            if matches!(message, Message::Close(_)) {
                break;
            }
        }
    });
    let (client, mut sub) = Client::connect(config(url)).expect("client");
    let delivered = sub.snapshots.recv().await.expect("snapshot");
    assert!(matches!(
        client
            .acknowledge_snapshot(Cursor("not-delivered".into()))
            .await,
        Err(ClientError::InvalidCursorAcknowledgement(_))
    ));
    client
        .acknowledge_snapshot(delivered.after_cursor)
        .await
        .expect("delivered snapshot ack");
    close_first_tx.send(()).expect("close signal");
    second_ready_rx.await.expect("second hello");
    client.close().await.expect("close");
    peer.await.expect("peer");
}

#[tokio::test]
async fn event_acknowledgements_require_delivery_and_monotonic_progress() {
    let (listener, url) = listener().await;
    let (send_events_tx, send_events_rx) = tokio::sync::oneshot::channel();
    let (close_first_tx, close_first_rx) = tokio::sync::oneshot::channel();
    let (second_ready_tx, second_ready_rx) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut first, hello) = accept_hello(&listener).await;
        hello_ok(&mut first, hello, hello_result(snapshot("base"))).await;
        send_events_rx.await.expect("event signal");
        for n in 1..=2 {
            let notification = json!({
                "jsonrpc":"2.0", "method":"attention.change", "params":{"event":{
                    "id":format!("event-{n}"), "cursor":format!("cursor-{n}"),
                    "occurred_at":"2026-01-01T00:00:00.000000Z", "kind":"work_item_created",
                    "affected":[], "inbox":{"upserts":[],"removals":[]}
                }}
            });
            first
                .send(Message::Text(notification.to_string().into()))
                .await
                .expect("event");
        }
        close_first_rx.await.expect("close signal");
        first.close(None).await.expect("close first");

        let (mut second, hello) = accept_hello(&listener).await;
        let params: protocol::HelloRequest =
            serde_json::from_value(hello.params.clone().expect("params")).expect("params type");
        assert!(matches!(
            params.subscription,
            SubscriptionRequest::Resume {
                after_cursor: Cursor(ref cursor),
                ..
            } if cursor == "cursor-2"
        ));
        hello_ok(
            &mut second,
            hello,
            hello_result(SubscriptionResult::Resume {
                after_cursor: Cursor("cursor-2".into()),
            }),
        )
        .await;
        second_ready_tx.send(()).expect("second hello signal");
        while let Some(Ok(message)) = second.next().await {
            if matches!(message, Message::Close(_)) {
                break;
            }
        }
    });
    let (client, mut sub) = Client::connect(config(url)).expect("client");
    let delivered_snapshot = sub.snapshots.recv().await.expect("snapshot");
    client
        .acknowledge_snapshot(delivered_snapshot.after_cursor)
        .await
        .expect("snapshot ack");
    send_events_tx.send(()).expect("event signal");
    let first = sub.changes.recv().await.expect("first event");
    let second = sub.changes.recv().await.expect("second event");
    assert_eq!(first.cursor, Cursor("cursor-1".into()));
    assert_eq!(second.cursor, Cursor("cursor-2".into()));
    assert!(matches!(
        client
            .acknowledge_cursor(Cursor("not-delivered".into()))
            .await,
        Err(ClientError::InvalidCursorAcknowledgement(_))
    ));
    client
        .acknowledge_cursor(second.cursor.clone())
        .await
        .expect("latest event ack");
    assert!(matches!(
        client.acknowledge_cursor(first.cursor).await,
        Err(ClientError::InvalidCursorAcknowledgement(_))
    ));
    close_first_tx.send(()).expect("close signal");
    second_ready_rx.await.expect("second hello");
    client.close().await.expect("close");
    peer.await.expect("peer");
}
