#![expect(clippy::expect_used, reason = "integration test assertions")]

mod support;

use attention_client::Client;
use attention_client::ClientConfig;
use attention_client::ClientError;
use attention_client::ConnectionStatus;
use attention_kernel as k;
use attention_protocol as p;
use attention_server::ServerConfig;
use std::sync::Arc;
use std::sync::Mutex;
use support::Barrier;
use support::ChangeScript;
use support::SERVER_ID;
use support::STREAM_ID;
use support::ScriptedService;
use support::TestServer;
use support::WAIT;
use support::at;
use support::cursor;
use support::event;

fn client_config(server: &TestServer) -> ClientConfig {
    let mut config = ClientConfig::new(server.url());
    config.request_timeout = WAIT;
    config.heartbeat_interval = std::time::Duration::from_secs(30);
    config.reconnect_min = std::time::Duration::from_millis(10);
    config.reconnect_max = std::time::Duration::from_millis(20);
    config
}

async fn recv_change(
    subscription: &mut attention_client::Subscription,
    expected: &str,
) -> p::ChangeEvent {
    let change = tokio::time::timeout(WAIT, subscription.changes.recv())
        .await
        .expect("change timeout")
        .expect("change");
    assert_eq!(change.cursor.as_str(), expected);
    change
}

async fn recv_snapshot(
    subscription: &mut attention_client::Subscription,
    expected: &str,
) -> attention_client::Snapshot {
    let snapshot = tokio::time::timeout(WAIT, subscription.snapshots.recv())
        .await
        .expect("snapshot timeout")
        .expect("snapshot");
    assert_eq!(snapshot.after_cursor.as_str(), expected);
    snapshot
}

#[tokio::test]
async fn snapshot_race_is_closed_by_subscribe_before_snapshot() {
    let barrier = Arc::new(Barrier::default());
    let mut service = ScriptedService::empty(1);
    service.snapshot_barrier = Some(Arc::clone(&barrier));
    let server = TestServer::start(ServerConfig::default(), Arc::new(service)).await;
    let (client, mut subscription) = Client::connect(client_config(&server)).expect("client");
    barrier.wait_entered().await;
    assert_eq!(server.state.publications.publish(&event(2)).delivered, 1);
    barrier.release();
    let snapshot = recv_snapshot(&mut subscription, "1").await;
    client
        .acknowledge_snapshot(snapshot.after_cursor)
        .await
        .expect("snapshot ack");
    let change = recv_change(&mut subscription, "2").await;
    client
        .acknowledge_cursor(change.cursor)
        .await
        .expect("change ack");
    client.close().await.expect("client close");
    server.shutdown().await;
}

#[tokio::test]
async fn replay_race_and_paged_resume_continue_into_live() {
    let barrier = Arc::new(Barrier::default());
    let events = Arc::new(Mutex::new(vec![event(2), event(3)]));
    let mut service = ScriptedService::empty(1);
    service.changes = ChangeScript::Events(Arc::clone(&events));
    service.changes_barrier = Some(Arc::clone(&barrier));
    let config = ServerConfig {
        replay_page_size: 1,
        ..ServerConfig::default()
    };
    let server = TestServer::start(config, Arc::new(service)).await;
    let mut config = client_config(&server);
    config.subscription = p::SubscriptionRequest::Resume {
        server_id: p::ServerId(SERVER_ID.into()),
        stream_id: p::StreamId(STREAM_ID.into()),
        after_cursor: p::Cursor("1".into()),
    };
    let (client, mut subscription) = Client::connect(config).expect("client");
    barrier.wait_entered().await;
    events
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(event(4));
    assert_eq!(server.state.publications.publish(&event(4)).delivered, 1);
    barrier.release();
    for expected in ["2", "3", "4"] {
        let change = recv_change(&mut subscription, expected).await;
        client
            .acknowledge_cursor(change.cursor)
            .await
            .expect("change ack");
    }
    assert_eq!(server.state.publications.publish(&event(5)).delivered, 1);
    recv_change(&mut subscription, "5").await;
    client.close().await.expect("client close");
    server.shutdown().await;
}

#[tokio::test]
async fn expired_and_future_gaps_fall_back_to_snapshot() {
    for gap in [
        k::ChangeGap::Expired {
            requested_after: cursor(1),
            earliest_available: cursor(3),
            latest_available: cursor(5),
        },
        k::ChangeGap::Future {
            requested_after: cursor(9),
            latest_available: cursor(5),
        },
    ] {
        let mut service = ScriptedService::empty(5);
        service.changes = ChangeScript::Gap(gap);
        let server = TestServer::start(ServerConfig::default(), Arc::new(service)).await;
        let mut config = client_config(&server);
        config.subscription = p::SubscriptionRequest::Resume {
            server_id: p::ServerId(SERVER_ID.into()),
            stream_id: p::StreamId(STREAM_ID.into()),
            after_cursor: p::Cursor(
                match gap {
                    k::ChangeGap::Future { .. } => "9",
                    k::ChangeGap::Expired { .. } => "1",
                }
                .into(),
            ),
        };
        let (client, mut subscription) = Client::connect(config).expect("client");
        let issue = tokio::time::timeout(WAIT, subscription.issues.recv())
            .await
            .expect("issue timeout")
            .expect("gap issue");
        assert!(matches!(issue.error, ClientError::Peer(ref error) if error.code == p::CURSOR_GAP));
        recv_snapshot(&mut subscription, "5").await;
        client.close().await.expect("client close");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn publication_overflow_reconnects_and_resumes() {
    let events = Arc::new(Mutex::new(vec![event(2), event(3)]));
    let mut service = ScriptedService::empty(1);
    service.changes = ChangeScript::Events(events);
    let config = ServerConfig {
        publication_capacity: 1,
        ..ServerConfig::default()
    };
    let server = TestServer::start(config, Arc::new(service)).await;
    let (client, mut subscription) = Client::connect(client_config(&server)).expect("client");
    let snapshot = recv_snapshot(&mut subscription, "1").await;
    client
        .acknowledge_snapshot(snapshot.after_cursor)
        .await
        .expect("snapshot ack");
    let first = server.state.publications.publish(&event(2));
    let second = server.state.publications.publish(&event(3));
    assert_eq!(first.delivered, 1);
    assert_eq!(second.overflowed, 1);

    // Overflow closes the live subscription before the client necessarily
    // acknowledges cursor 2. A reconnect may therefore replay cursor 2; model
    // the reducer's idempotency boundary while requiring both cursors to be
    // applied in order and cursor 3 to be applied exactly once.
    let mut applied = Vec::new();
    while applied.len() < 2 {
        let change = tokio::time::timeout(WAIT, subscription.changes.recv())
            .await
            .expect("change timeout")
            .expect("change");
        let cursor = change.cursor.as_str().to_owned();
        assert!(matches!(cursor.as_str(), "2" | "3"));
        if applied.contains(&cursor) {
            continue;
        }
        assert_eq!(cursor, ["2", "3"][applied.len()]);
        applied.push(cursor);
        client
            .acknowledge_cursor(change.cursor)
            .await
            .expect("change ack");
    }
    assert_eq!(applied, ["2", "3"]);
    client.close().await.expect("client close");
    server.shutdown().await;
}

struct Fixtures {
    work_item: k::WorkItem,
    signal: k::AttentionSignal,
    reminder: k::Reminder,
    entity: k::SourceEntity,
    receipt: k::SourceReceipt,
}
fn fixtures() -> Fixtures {
    let kind = k::SourceKind::new("github").expect("kind");
    let instance = k::SourceInstance::new("acme").expect("instance");
    let occurrence = k::OccurrenceKey::new(
        kind.clone(),
        instance.clone(),
        k::OccurrenceId::new("issue-7/event-9").expect("occurrence"),
    );
    let key = k::SourceEntityKey::new(
        kind,
        instance,
        k::ExternalEntityId::new("issue-7").expect("external ID"),
    );
    let receipt_id = k::SourceReceiptId::new();
    let order = k::SourceOrderMode::Ordered {
        domain: k::SourceComparatorDomain::new("github.updated_at").expect("domain"),
        value: Some(k::NormalizedSourceOrder::new(vec![0, 1, 2, 3]).expect("order")),
    };
    let receipt = k::SourceReceipt::reconstruct(
        receipt_id,
        occurrence,
        Some(key.clone()),
        k::CanonicalFingerprint::reconstruct([7; 32]),
        order.clone(),
        at("2026-08-13T10:00:00Z"),
        at("2026-08-13T10:00:01Z"),
    )
    .expect("receipt");
    let entity = k::SourceEntity::reconstruct(
        k::SourceEntityId::new(),
        key.clone(),
        k::SourceStateVersion::initial(),
        receipt_id,
        order,
    )
    .expect("entity");
    let work_item = k::WorkItem::new(
        k::WorkItemId::new(),
        Some(at("2026-08-14T00:00:00Z")),
        None,
        Some(at("2026-08-13T20:00:00Z")),
        Some(key),
    );
    let signal =
        k::AttentionSignal::new(k::AttentionSignalId::new(), receipt_id, Some(entity.id()));
    let reminder = k::Reminder::new(
        k::ReminderId::new(),
        k::ReminderTarget::WorkItem(work_item.id()),
        at("2026-08-13T18:00:00Z"),
        k::ReminderFireId::new(),
    );
    Fixtures {
        work_item,
        signal,
        reminder,
        entity,
        receipt,
    }
}

#[tokio::test]
async fn all_five_point_reads_and_rich_source_receipt_round_trip() {
    let fixture = fixtures();
    let mut service = ScriptedService::empty(1);
    service.work_item = Some(fixture.work_item.clone());
    service.signal = Some(fixture.signal.clone());
    service.reminder = Some(fixture.reminder.clone());
    service.entity = Some(fixture.entity.clone());
    service.receipt = Some(fixture.receipt.clone());
    let server = TestServer::start(ServerConfig::default(), Arc::new(service)).await;
    let mut config = client_config(&server);
    config.subscription = p::SubscriptionRequest::None;
    let (client, _subscription) = Client::connect(config).expect("client");
    assert_eq!(
        client
            .work_item_get(p::WorkItemGetParams {
                id: p::WorkItemId(fixture.work_item.id().to_string())
            })
            .await
            .expect("work item")
            .id
            .0,
        fixture.work_item.id().to_string()
    );
    assert_eq!(
        client
            .attention_signal_get(p::AttentionSignalGetParams {
                id: p::AttentionSignalId(fixture.signal.id().to_string())
            })
            .await
            .expect("signal")
            .id
            .0,
        fixture.signal.id().to_string()
    );
    assert_eq!(
        client
            .reminder_get(p::ReminderGetParams {
                id: p::ReminderId(fixture.reminder.id().to_string())
            })
            .await
            .expect("reminder")
            .id
            .0,
        fixture.reminder.id().to_string()
    );
    let entity = client
        .source_entity_get(p::SourceEntityGetParams {
            key: p::SourceEntityKey {
                source_kind: p::SourceKind("github".into()),
                source_instance: p::SourceInstance("acme".into()),
                external_entity_id: p::ExternalEntityId("issue-7".into()),
            },
        })
        .await
        .expect("entity");
    assert_eq!(entity.id.0, fixture.entity.id().to_string());
    let receipt = client
        .source_receipt_get(p::SourceReceiptGetParams {
            id: p::SourceReceiptId(fixture.receipt.id().to_string()),
        })
        .await
        .expect("receipt");
    assert_eq!(
        receipt.occurrence_key.occurrence_id.as_str(),
        "issue-7/event-9"
    );
    assert!(receipt.source_entity_key.is_some());
    assert!(matches!(
        receipt.source_order,
        p::SourceOrder::Ordered { value: Some(_), .. }
    ));
    assert_eq!(
        receipt.occurred_at.as_datetime(),
        &at("2026-08-13T10:00:00Z")
    );
    assert_eq!(
        receipt.ingested_at.as_datetime(),
        &at("2026-08-13T10:00:01Z")
    );
    client.close().await.expect("client close");
    server.shutdown().await;
}

struct EmptyDeliveryWorkers;

impl attention_server::DeliveryWorkerService for EmptyDeliveryWorkers {
    fn claim(
        &self,
        _: k::DeliveryClaimQuery,
    ) -> futures_util::future::BoxFuture<
        '_,
        Result<Vec<k::DeliveryClaim>, attention_server::ServiceError>,
    > {
        Box::pin(async { Ok(vec![]) })
    }

    fn inspect(
        &self,
        _: k::OutboxIntentId,
    ) -> futures_util::future::BoxFuture<
        '_,
        Result<Option<k::DeliveryAuthority>, attention_server::ServiceError>,
    > {
        Box::pin(async { Ok(None) })
    }

    fn renew(
        &self,
        _: k::OutboxIntentId,
        _: k::DeliveryLeaseToken,
        _: chrono::DateTime<chrono::Utc>,
    ) -> futures_util::future::BoxFuture<'_, Result<k::RenewOutcome, attention_server::ServiceError>>
    {
        Box::pin(async { panic!("renew is not used by this test") })
    }

    fn succeed(
        &self,
        _: k::OutboxIntentId,
        _: k::DeliveryLeaseToken,
        _: k::ProviderMessageId,
        _: chrono::DateTime<chrono::Utc>,
    ) -> futures_util::future::BoxFuture<
        '_,
        Result<k::DeliveryCompletionOutcome, attention_server::ServiceError>,
    > {
        Box::pin(async { panic!("succeed is not used by this test") })
    }

    fn fail_retryable(
        &self,
        _: k::OutboxIntentId,
        _: k::DeliveryLeaseToken,
        _: u32,
        _: k::BoundedDeliveryText,
        _: chrono::DateTime<chrono::Utc>,
    ) -> futures_util::future::BoxFuture<
        '_,
        Result<k::DeliveryCompletionOutcome, attention_server::ServiceError>,
    > {
        Box::pin(async { panic!("retryable failure is not used by this test") })
    }

    fn fail_terminal(
        &self,
        _: k::OutboxIntentId,
        _: k::DeliveryLeaseToken,
        _: u32,
        _: k::BoundedDeliveryText,
        _: chrono::DateTime<chrono::Utc>,
    ) -> futures_util::future::BoxFuture<
        '_,
        Result<k::DeliveryCompletionOutcome, attention_server::ServiceError>,
    > {
        Box::pin(async { panic!("terminal failure is not used by this test") })
    }
}

#[tokio::test]
async fn composed_delivery_worker_method_returns_typed_result() {
    let server = TestServer::start_with_delivery_workers(
        ServerConfig::default(),
        Arc::new(ScriptedService::empty(1)),
        Arc::new(EmptyDeliveryWorkers),
    )
    .await;
    let mut config = client_config(&server);
    config.subscription = p::SubscriptionRequest::None;
    let (client, _) = Client::connect(config).expect("client");
    let intent_id = p::OutboxIntentId(k::OutboxIntentId::new().to_string());
    let error = client
        .delivery_inspect(p::DeliveryInspectParams {
            intent_id: intent_id.clone(),
        })
        .await
        .expect_err("missing delivery");
    assert!(matches!(error, ClientError::Peer(ref peer) if peer.code == p::DELIVERY_NOT_FOUND));
    let typed = match error {
        ClientError::Peer(peer) => p::V1Error::try_from(peer).expect("typed peer error"),
        other => panic!("unexpected client error: {other}"),
    };
    assert!(matches!(
        typed,
        p::V1Error::DeliveryNotFound(ref data) if data.intent_id == intent_id
    ));
    client.close().await.expect("client close");
    server.shutdown().await;
}

#[tokio::test]
async fn server_shutdown_closes_sdk_supervisor() {
    let server =
        TestServer::start(ServerConfig::default(), Arc::new(ScriptedService::empty(1))).await;
    let (client, mut subscription) = Client::connect(client_config(&server)).expect("client");
    recv_snapshot(&mut subscription, "1").await;
    server.state.shutdown.cancel();
    let mut status = client.status();
    tokio::time::timeout(WAIT, async {
        loop {
            status.changed().await.expect("status channel");
            if matches!(*status.borrow(), ConnectionStatus::Reconnecting { .. }) {
                break;
            }
        }
    })
    .await
    .expect("reconnect status timeout");
    client.close().await.expect("client close");
    server.shutdown().await;
}

#[test]
fn loopback_and_origin_configuration_is_strict() {
    let mut config = ServerConfig {
        bind: "0.0.0.0:9000".parse().expect("address"),
        ..ServerConfig::default()
    };
    assert!(config.validate().is_err());
    config.allow_non_loopback = true;
    config
        .allowed_origins
        .insert("https://example.com/path".into());
    assert!(config.validate().is_err());
    config.allowed_origins.clear();
    config.allowed_origins.insert("https://example.com".into());
    assert!(config.validate().is_ok());
}
