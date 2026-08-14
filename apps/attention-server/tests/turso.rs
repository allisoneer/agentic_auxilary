#![expect(clippy::expect_used, reason = "integration test assertions")]

use attention_client::Client;
use attention_client::ClientConfig;
use attention_client::ClientError;
use attention_kernel as k;
use attention_protocol as p;
use attention_server::RuntimeHandle;
use attention_server::ServerConfig;
use attention_server::runtime;
use attention_turso::AttentionDatabase;
use attention_turso::Config as TursoConfig;
use chrono::DateTime;
use chrono::Utc;
use futures_util::SinkExt;
use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use serde_json::Value;
use serde_json::json;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing_subscriber::fmt::MakeWriter;
use turso_db::Builder;
use turso_db::params;

const WAIT: Duration = Duration::from_secs(5);
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<Vec<u8>>>);
struct LogWriter(Arc<Mutex<Vec<u8>>>);
impl io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("log lock").extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl<'a> MakeWriter<'a> for LogCapture {
    type Writer = LogWriter;
    fn make_writer(&'a self) -> Self::Writer {
        LogWriter(Arc::clone(&self.0))
    }
}
impl LogCapture {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("log lock")).into_owned()
    }
}

struct Fixture {
    _root: TempDir,
    database: PathBuf,
    backups: PathBuf,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let root = tempfile::tempdir()?;
        Ok(Self {
            database: root.path().join("database"),
            backups: root.path().join("backups"),
            _root: root,
        })
    }

    fn turso(&self) -> TestResult<TursoConfig> {
        Ok(TursoConfig::new(&self.database, &self.backups)?)
    }

    async fn start(&self) -> TestResult<RuntimeHandle> {
        self.start_with(ServerConfig::default()).await
    }

    async fn start_with(&self, config: ServerConfig) -> TestResult<RuntimeHandle> {
        let turso = self.turso()?;
        Ok(runtime::start(config, turso).await?)
    }
}

#[derive(Clone)]
struct ManualTime {
    now: Arc<Mutex<DateTime<Utc>>>,
    wake: Arc<Notify>,
    sleep_calls: Arc<AtomicUsize>,
}
impl ManualTime {
    fn new(value: &str) -> Self {
        Self {
            now: Arc::new(Mutex::new(
                DateTime::parse_from_rfc3339(value)
                    .expect("manual timestamp")
                    .with_timezone(&Utc),
            )),
            wake: Arc::new(Notify::new()),
            sleep_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn advance(&self, value: &str) {
        *self.now.lock().expect("clock lock") = DateTime::parse_from_rfc3339(value)
            .expect("manual timestamp")
            .with_timezone(&Utc);
        self.wake.notify_waiters();
    }
    fn sleep_calls(&self) -> usize {
        self.sleep_calls.load(Ordering::SeqCst)
    }
}
impl attention_server::Clock for ManualTime {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().expect("clock lock")
    }
}
impl attention_server::Sleeper for ManualTime {
    fn sleep(&self, _duration: Duration) -> BoxFuture<'static, ()> {
        self.sleep_calls.fetch_add(1, Ordering::SeqCst);
        let wake = Arc::clone(&self.wake);
        Box::pin(async move { wake.notified().await })
    }
}

fn client_config(handle: &RuntimeHandle, subscription: p::SubscriptionRequest) -> ClientConfig {
    let mut config = ClientConfig::new(format!("ws://{}/v1/ws", handle.address()));
    config.subscription = subscription;
    config.request_timeout = WAIT;
    config.heartbeat_interval = Duration::from_secs(30);
    config.reconnect_min = Duration::from_millis(10);
    config.reconnect_max = Duration::from_millis(20);
    config
}

fn key() -> p::MutationIdempotencyKey {
    p::MutationIdempotencyKey(k::MutationIdempotencyKey::new().to_string())
}
fn work_id() -> p::WorkItemId {
    p::WorkItemId(k::WorkItemId::new().to_string())
}
fn revision(value: &str) -> p::Revision {
    p::Revision::parse(value).expect("revision")
}
fn timestamp(value: &str) -> p::WireTimestamp {
    p::WireTimestamp::parse(value).expect("timestamp")
}
fn create_params(
    id: p::WorkItemId,
    idempotency_key: p::MutationIdempotencyKey,
) -> p::CreateWorkItemParams {
    p::CreateWorkItemParams {
        id,
        due_at: None,
        scheduled_at: None,
        defer_until: None,
        source_link: None,
        idempotency_key,
    }
}
fn peer_code(error: ClientError) -> p::ErrorCode {
    match error {
        ClientError::Peer(peer) => peer.code,
        other => panic!("expected peer error, got {other:?}"),
    }
}
async fn next_change(subscription: &mut attention_client::Subscription) -> p::ChangeEvent {
    tokio::time::timeout(WAIT, async {
        tokio::select! {
            change = subscription.changes.recv() => change.expect("change"),
            issue = subscription.issues.recv() => panic!("client issue before change: {:?}", issue.expect("issue").error),
        }
    })
    .await
    .expect("change timeout")
}

#[tokio::test]
async fn empty_startup_migrates_before_bind_and_bad_checksum_prevents_start() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    handle.shutdown().await?;

    let config = fixture.turso()?;
    let path = config.database_directory().database_file();
    let database = Builder::new_local(path.to_str().ok_or("database path encoding")?)
        .build()
        .await?;
    let connection = database.connect()?;
    connection
        .execute(
            "UPDATE __attention_migrations SET checksum = ?1 WHERE version = 1",
            params![vec![0_u8; 32]],
        )
        .await?;
    drop(connection);
    drop(database);

    let error = match runtime::start(ServerConfig::default(), fixture.turso()?).await {
        Ok(handle) => {
            handle.shutdown().await?;
            panic!("checksum drift must prevent startup");
        }
        Err(error) => error,
    };
    assert!(error.to_string().contains("database startup failed"));
    Ok(())
}

#[tokio::test]
async fn identity_restart_mutations_replay_conflicts_and_exact_events() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let first_identity = handle.identity().clone();
    let (client, mut subscription) =
        Client::connect(client_config(&handle, p::SubscriptionRequest::Snapshot))?;
    let snapshot = tokio::time::timeout(WAIT, subscription.snapshots.recv())
        .await?
        .ok_or("snapshot")?;
    client.acknowledge_snapshot(snapshot.after_cursor).await?;

    let id = work_id();
    let replay_key = key();
    let create = create_params(id.clone(), replay_key.clone());
    let applied = client.work_item_create(create.clone()).await?;
    assert_eq!(applied.disposition, p::MutationDisposition::Applied);
    // The RPC response is emitted only after commit, so state is queryable even
    // before the independently queued publication is consumed.
    let queried = client
        .work_item_get(p::WorkItemGetParams { id: id.clone() })
        .await?;
    assert_eq!(queried.lifecycle, p::WorkItemLifecycle::Open);
    let event = next_change(&mut subscription).await;
    assert_eq!(event.cursor, applied.cursor);
    assert_eq!(event.id, applied.change_event_id);
    assert_eq!(event.kind, p::ChangeKind::WorkItemCreated);

    let second_id = work_id();
    let second = client
        .work_item_create(create_params(second_id, key()))
        .await?;
    assert_eq!(next_change(&mut subscription).await.cursor, second.cursor);
    let replayed = client.work_item_create(create.clone()).await?;
    assert_eq!(replayed.disposition, p::MutationDisposition::Replayed);
    assert_eq!(replayed.cursor, applied.cursor);
    assert_eq!(replayed.change_event_id, applied.change_event_id);
    // Replay republishes the frozen historical event, which an already-ahead
    // ordered subscription must suppress rather than expose as a duplicate or
    // cursor regression.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), subscription.changes.recv())
            .await
            .is_err()
    );
    let history = client
        .changes_get(p::ChangesGetParams {
            after_cursor: p::Cursor("1".into()),
            limit: 16,
        })
        .await?;
    let p::ChangesResult::Page { events, .. } = history else {
        panic!("history unexpectedly returned a gap");
    };
    let frozen = events
        .iter()
        .find(|candidate| candidate.cursor == applied.cursor)
        .expect("historical event");
    assert_eq!(frozen.id, applied.change_event_id);
    assert_eq!(frozen, &event);
    let mut changed = create;
    changed.due_at = Some(timestamp("2026-08-14T00:00:00.000000Z"));
    assert_eq!(
        peer_code(
            client
                .work_item_create(changed)
                .await
                .expect_err("mismatch")
        ),
        p::IDEMPOTENCY_MISMATCH
    );

    assert_eq!(
        peer_code(
            client
                .work_item_complete(p::CompleteWorkItemParams {
                    id: id.clone(),
                    expected_revision: revision("9"),
                    idempotency_key: key(),
                })
                .await
                .expect_err("revision conflict")
        ),
        p::EXPECTED_REVISION_CONFLICT
    );
    let completed = client
        .work_item_complete(p::CompleteWorkItemParams {
            id: id.clone(),
            expected_revision: revision("1"),
            idempotency_key: key(),
        })
        .await?;
    assert!(completed.cursor.as_str().parse::<u64>()? > applied.cursor.as_str().parse::<u64>()?);
    assert_eq!(
        client
            .work_item_get(p::WorkItemGetParams { id: id.clone() })
            .await?
            .lifecycle,
        p::WorkItemLifecycle::Completed
    );

    let cancel_id = work_id();
    client
        .work_item_create(create_params(cancel_id.clone(), key()))
        .await?;
    client
        .work_item_cancel(p::CancelWorkItemParams {
            id: cancel_id.clone(),
            expected_revision: revision("1"),
            idempotency_key: key(),
        })
        .await?;
    assert_eq!(
        client
            .work_item_get(p::WorkItemGetParams { id: cancel_id })
            .await?
            .lifecycle,
        p::WorkItemLifecycle::Cancelled
    );
    client.close().await?;
    handle.shutdown().await?;

    let restarted = fixture.start().await?;
    assert_eq!(restarted.identity().server_id, first_identity.server_id);
    assert_eq!(restarted.identity().stream_id, first_identity.stream_id);
    assert_ne!(restarted.identity().boot_id, first_identity.boot_id);
    let (client, _) = Client::connect(client_config(&restarted, p::SubscriptionRequest::None))?;
    assert_eq!(
        client
            .work_item_get(p::WorkItemGetParams { id })
            .await?
            .lifecycle,
        p::WorkItemLifecycle::Completed
    );
    client.close().await?;
    restarted.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn independent_clients_racing_one_revision_commit_one_authoritative_event() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let (complete_client, _) =
        Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;
    let (cancel_client, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;

    let id = work_id();
    let created = complete_client
        .work_item_create(create_params(id.clone(), key()))
        .await?;
    let complete = complete_client.work_item_complete(p::CompleteWorkItemParams {
        id: id.clone(),
        expected_revision: revision("1"),
        idempotency_key: key(),
    });
    let cancel = cancel_client.work_item_cancel(p::CancelWorkItemParams {
        id: id.clone(),
        expected_revision: revision("1"),
        idempotency_key: key(),
    });
    let (complete_result, cancel_result) = tokio::join!(complete, cancel);

    let (winner_cursor, expected_lifecycle, expected_kind, loser) =
        match (complete_result, cancel_result) {
            (Ok(winner), Err(loser)) => (
                winner.cursor,
                p::WorkItemLifecycle::Completed,
                p::ChangeKind::WorkItemCompleted,
                loser,
            ),
            (Err(loser), Ok(winner)) => (
                winner.cursor,
                p::WorkItemLifecycle::Cancelled,
                p::ChangeKind::WorkItemCancelled,
                loser,
            ),
            (complete, cancel) => panic!(
                "exactly one racing mutation must win: complete={complete:?}, cancel={cancel:?}"
            ),
        };
    assert_eq!(peer_code(loser), p::EXPECTED_REVISION_CONFLICT);

    let authoritative = complete_client
        .work_item_get(p::WorkItemGetParams { id: id.clone() })
        .await?;
    assert_eq!(authoritative.id, id);
    assert_eq!(authoritative.revision, revision("2"));
    assert_eq!(authoritative.lifecycle, expected_lifecycle);

    let history = complete_client
        .changes_get(p::ChangesGetParams {
            after_cursor: created.cursor,
            limit: 10,
        })
        .await?;
    let p::ChangesResult::Page { events, .. } = history else {
        panic!("race history unexpectedly returned a gap");
    };
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].cursor, winner_cursor);
    assert_eq!(events[0].kind, expected_kind);
    assert!(events[0].inbox.upserts.is_empty());
    assert!(matches!(
        events[0].inbox.removals.as_slice(),
        [p::InboxEntryKey::WorkItem { work_item_id }] if work_item_id == &id
    ));
    let snapshot = complete_client.snapshot_get(p::EmptyParams {}).await?;
    let snapshot_item = snapshot
        .state
        .work_items
        .iter()
        .find(|item| item.id == id)
        .expect("authoritative snapshot item");
    assert_eq!(snapshot_item.lifecycle, expected_lifecycle);
    assert!(snapshot.state.inbox.iter().all(
        |entry| !matches!(entry, p::InboxEntryView::WorkItem { work_item } if work_item.id == id)
    ));

    complete_client.close().await?;
    cancel_client.close().await?;
    handle.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn stopped_backup_restore_preserves_domain_history_identity_and_future_gap() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let identity = handle.identity().clone();
    let (client, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;

    let id = work_id();
    let created = client
        .work_item_create(create_params(id.clone(), key()))
        .await?;
    let completed = client
        .work_item_complete(p::CompleteWorkItemParams {
            id: id.clone(),
            expected_revision: revision("1"),
            idempotency_key: key(),
        })
        .await?;
    assert!(completed.cursor.as_str().parse::<u64>()? > created.cursor.as_str().parse::<u64>()?);
    client.close().await?;
    handle.shutdown().await?;

    // Backup is deliberately a stopped operation. Reopen only the adapter, close it so all
    // engine/connection/ownership handles are dropped, then copy the complete manifested set.
    let stopped = AttentionDatabase::open(fixture.turso()?).await?;
    assert_eq!(stopped.run_startup_migrations().await?.applied(), 0);
    stopped.close().await?;
    let manifest = stopped.backup("domain")?;
    assert_eq!(manifest.format_version(), 2);
    assert_eq!(manifest.migration_head(), 5);
    assert_eq!(manifest.payload_version(), 1);
    assert!(!manifest.files().is_empty());
    assert!(
        manifest
            .files()
            .windows(2)
            .all(|pair| pair[0].path() < pair[1].path())
    );
    assert!(
        manifest
            .files()
            .iter()
            .all(|entry| entry.size() > 0 && entry.checksum().len() == 32)
    );
    drop(stopped);

    // Produce a real client cursor that is newer than the stopped backup's event tail.
    let advanced = fixture.start().await?;
    let (client, _) = Client::connect(client_config(&advanced, p::SubscriptionRequest::None))?;
    let beyond_backup = client
        .work_item_create(create_params(work_id(), key()))
        .await?
        .cursor;
    assert!(beyond_backup.as_str().parse::<u64>()? > completed.cursor.as_str().parse::<u64>()?);
    client.close().await?;
    advanced.shutdown().await?;

    let restore_root = tempfile::tempdir()?;
    let restored_config = TursoConfig::new(
        restore_root.path().join("database"),
        fixture.backups.clone(),
    )?;
    let restored_database = AttentionDatabase::restore(restored_config.clone(), "domain").await?;
    assert_eq!(
        restored_database.run_startup_migrations().await?.applied(),
        0
    );
    restored_database.close().await?;
    drop(restored_database);

    let restored = runtime::start(ServerConfig::default(), restored_config).await?;
    assert_eq!(restored.identity().server_id, identity.server_id);
    assert_eq!(restored.identity().stream_id, identity.stream_id);
    assert_ne!(restored.identity().boot_id, identity.boot_id);
    let (client, _) = Client::connect(client_config(&restored, p::SubscriptionRequest::None))?;
    let work = client
        .work_item_get(p::WorkItemGetParams { id: id.clone() })
        .await?;
    assert_eq!(work.lifecycle, p::WorkItemLifecycle::Completed);
    let history = client
        .changes_get(p::ChangesGetParams {
            after_cursor: p::Cursor("1".into()),
            limit: 16,
        })
        .await?;
    let p::ChangesResult::Page { events, .. } = history else {
        panic!("restored history unexpectedly returned a gap");
    };
    assert!(events.iter().any(|event| {
        event.cursor == created.cursor && event.kind == p::ChangeKind::WorkItemCreated
    }));
    assert!(events.iter().any(|event| {
        event.cursor == completed.cursor && event.kind == p::ChangeKind::WorkItemCompleted
    }));
    client.close().await?;

    let future = p::SubscriptionRequest::Resume {
        server_id: identity.server_id,
        stream_id: identity.stream_id,
        after_cursor: beyond_backup.clone(),
    };
    let (future_client, mut future_subscription) =
        Client::connect(client_config(&restored, future))?;
    let issue = tokio::time::timeout(WAIT, future_subscription.issues.recv())
        .await?
        .ok_or("restored future gap")?;
    let ClientError::Peer(peer) = issue.error else {
        panic!("restored future cursor did not return a peer error");
    };
    let p::V1Error::CursorGap(gap) = p::V1Error::try_from(peer)? else {
        panic!("restored future cursor did not return a typed cursor gap");
    };
    assert_eq!(gap.reason, p::CursorGapReason::Future);
    assert_eq!(gap.requested_after, beyond_backup);
    assert_eq!(gap.latest_available, Some(completed.cursor));
    future_client.close().await?;
    restored.shutdown().await?;
    Ok(())
}

fn ingest(
    receipt: p::SourceReceiptId,
    signal: p::AttentionSignalId,
    entity: p::SourceEntityIdentity,
    occurrence: &str,
    order: p::SourceOrder,
    idempotency_key: p::MutationIdempotencyKey,
) -> p::IngestSourceOccurrenceParams {
    p::IngestSourceOccurrenceParams {
        receipt_id: receipt,
        entity: Some(entity),
        signal_id: signal,
        occurrence_key: p::OccurrenceKey {
            source_kind: p::SourceKind("github".into()),
            source_instance: p::SourceInstance("acme".into()),
            occurrence_id: p::OccurrenceId(occurrence.into()),
        },
        occurred_at: timestamp("2026-08-13T10:00:00.000000Z"),
        order,
        source_lifecycle: p::SignalSourceLifecycle::Active,
        fresh_attention: true,
        idempotency_key,
    }
}

fn ordered(domain: &str, value: Option<&str>) -> p::SourceOrder {
    p::SourceOrder::Ordered {
        domain: p::SourceOrderDomain(domain.into()),
        value: value.map(|value| p::NormalizedSourceOrder::parse(value).expect("order")),
    }
}

#[tokio::test]
async fn source_dedupe_order_receipts_and_reminders_survive_restart() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let (client, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;
    let entity = p::SourceEntityIdentity {
        id: p::SourceEntityId(k::SourceEntityId::new().to_string()),
        key: p::SourceEntityKey {
            source_kind: p::SourceKind("github".into()),
            source_instance: p::SourceInstance("acme".into()),
            external_entity_id: p::ExternalEntityId("issue-7".into()),
        },
    };
    let signal = p::AttentionSignalId(k::AttentionSignalId::new().to_string());
    let receipt1 = p::SourceReceiptId(k::SourceReceiptId::new().to_string());
    let first = ingest(
        receipt1.clone(),
        signal.clone(),
        entity.clone(),
        "event-1",
        ordered("updated_at", Some("Ag")),
        key(),
    );
    let first_result = client.source_occurrence_ingest(first.clone()).await?;
    assert_eq!(
        first_result.value.decision,
        p::SourceIngestionDecision::Advanced
    );
    assert_eq!(
        client.source_occurrence_ingest(first).await?.disposition,
        p::MutationDisposition::Replayed
    );
    let receipt2 = p::SourceReceiptId(k::SourceReceiptId::new().to_string());
    let older = client
        .source_occurrence_ingest(ingest(
            receipt2.clone(),
            signal.clone(),
            entity.clone(),
            "event-2",
            ordered("updated_at", Some("AQ")),
            key(),
        ))
        .await?;
    assert!(matches!(
        older.value.decision,
        p::SourceIngestionDecision::ReceiptOnly {
            reason: p::ReceiptOnlyReason::Older
        }
    ));
    let mut receipt_ids = vec![receipt1.clone(), receipt2.clone()];
    for (occurrence, order, reason) in [
        (
            "event-equal",
            ordered("updated_at", Some("Ag")),
            p::ReceiptOnlyReason::Equal,
        ),
        (
            "event-missing",
            ordered("updated_at", None),
            p::ReceiptOnlyReason::MissingOrderedValue,
        ),
        (
            "event-domain",
            ordered("sequence", Some("Aw")),
            p::ReceiptOnlyReason::ComparatorDomainMismatch,
        ),
        (
            "event-unordered",
            p::SourceOrder::Unordered,
            p::ReceiptOnlyReason::Incomparable,
        ),
    ] {
        let receipt = p::SourceReceiptId(k::SourceReceiptId::new().to_string());
        let result = client
            .source_occurrence_ingest(ingest(
                receipt.clone(),
                signal.clone(),
                entity.clone(),
                occurrence,
                order,
                key(),
            ))
            .await?;
        assert_eq!(
            result.value.decision,
            p::SourceIngestionDecision::ReceiptOnly { reason }
        );
        receipt_ids.push(receipt);
    }
    let changed = ingest(
        p::SourceReceiptId(k::SourceReceiptId::new().to_string()),
        signal,
        entity,
        "event-1",
        ordered("updated_at", Some("Aw")),
        key(),
    );
    assert_eq!(
        peer_code(
            client
                .source_occurrence_ingest(changed)
                .await
                .expect_err("occurrence mismatch")
        ),
        p::OCCURRENCE_CONTENT_MISMATCH
    );

    let work = work_id();
    client
        .work_item_create(create_params(work.clone(), key()))
        .await?;
    let reminder = p::ReminderId(k::ReminderId::new().to_string());
    let fire = p::ReminderFireId(k::ReminderFireId::new().to_string());
    client
        .reminder_create(p::CreateReminderParams {
            reminder_id: reminder.clone(),
            initial_fire_id: fire.clone(),
            target: p::ReminderTarget::WorkItem { work_item_id: work },
            trigger_at: timestamp("2026-08-14T00:00:00.000000Z"),
            idempotency_key: key(),
        })
        .await?;
    assert_eq!(
        peer_code(
            client
                .reminder_fire_acknowledge(p::AcknowledgeReminderFireParams {
                    reminder_id: reminder.clone(),
                    fire_id: fire.clone(),
                    expected_revision: revision("1"),
                    idempotency_key: key(),
                })
                .await
                .expect_err("scheduled fire cannot acknowledge")
        ),
        p::INVALID_PARAMS
    );
    let scheduled_snooze = client
        .reminder_fire_snooze(p::SnoozeReminderFireParams {
            reminder_id: reminder.clone(),
            fire_id: fire.clone(),
            replacement_fire_id: p::ReminderFireId(k::ReminderFireId::new().to_string()),
            replacement_trigger_at: timestamp("2026-08-15T00:00:00.000000Z"),
            expected_revision: revision("1"),
            idempotency_key: key(),
        })
        .await
        .expect_err("scheduled fire cannot snooze");
    assert_eq!(peer_code(scheduled_snooze), p::INVALID_PARAMS);
    let acknowledge_work = work_id();
    client
        .work_item_create(create_params(acknowledge_work.clone(), key()))
        .await?;
    let acknowledge_reminder = p::ReminderId(k::ReminderId::new().to_string());
    let acknowledge_fire = p::ReminderFireId(k::ReminderFireId::new().to_string());
    client
        .reminder_create(p::CreateReminderParams {
            reminder_id: acknowledge_reminder.clone(),
            initial_fire_id: acknowledge_fire.clone(),
            target: p::ReminderTarget::WorkItem {
                work_item_id: acknowledge_work,
            },
            trigger_at: timestamp("2026-08-14T00:00:00.000000Z"),
            idempotency_key: key(),
        })
        .await?;
    client.close().await?;
    handle.shutdown().await?;

    // Narrow fixture hook: the scheduler is outside T09. Mark scheduled
    // children fired, then prove source-ingress outbox/delivery atomicity.
    let config = fixture.turso()?;
    let path = config.database_directory().database_file();
    let database = Builder::new_local(path.to_str().ok_or("database path encoding")?)
        .build()
        .await?;
    let connection = database.connect()?;
    connection
        .execute(
            "UPDATE reminder_fires SET state = 1 WHERE id IN (?1, ?2)",
            params![fire.as_str(), acknowledge_fire.as_str()],
        )
        .await?;
    let mut rows = connection
        .query(
            "SELECT (SELECT count(*) FROM outbox_intents), (SELECT count(*) FROM delivery_states)",
            (),
        )
        .await?;
    let row = rows.next().await?.ok_or("outbox inventory")?;
    assert_eq!(row.get::<i64>(0)?, 1);
    assert_eq!(row.get::<i64>(1)?, 1);
    drop(rows);
    drop(connection);
    drop(database);

    let restarted = fixture.start().await?;
    let (client, _) = Client::connect(client_config(&restarted, p::SubscriptionRequest::None))?;
    for receipt_id in receipt_ids {
        let receipt = client
            .source_receipt_get(p::SourceReceiptGetParams { id: receipt_id })
            .await?;
        assert_eq!(receipt.occurrence_key.source_kind.as_str(), "github");
        assert_eq!(receipt.occurrence_key.source_instance.as_str(), "acme");
        assert_eq!(
            receipt
                .source_entity_key
                .expect("entity key")
                .external_entity_id
                .as_str(),
            "issue-7"
        );
        assert_eq!(
            receipt.occurred_at,
            timestamp("2026-08-13T10:00:00.000000Z")
        );
        assert!(receipt.ingested_at >= receipt.occurred_at);
    }
    assert_eq!(
        client
            .reminder_fire_snooze(p::SnoozeReminderFireParams {
                reminder_id: reminder.clone(),
                fire_id: fire,
                replacement_fire_id: p::ReminderFireId(k::ReminderFireId::new().to_string()),
                replacement_trigger_at: timestamp("2026-08-15T00:00:00.000000Z"),
                expected_revision: revision("1"),
                idempotency_key: key(),
            })
            .await?
            .disposition,
        p::MutationDisposition::Applied
    );
    assert_eq!(
        client
            .reminder_fire_acknowledge(p::AcknowledgeReminderFireParams {
                reminder_id: acknowledge_reminder.clone(),
                fire_id: acknowledge_fire,
                expected_revision: revision("1"),
                idempotency_key: key(),
            })
            .await?
            .disposition,
        p::MutationDisposition::Applied
    );
    assert_eq!(
        client
            .reminder_get(p::ReminderGetParams {
                id: acknowledge_reminder,
            })
            .await?
            .fires[0]
            .state,
        p::ReminderFireState::Acknowledged
    );
    assert_eq!(
        client
            .reminder_get(p::ReminderGetParams {
                id: reminder.clone(),
            })
            .await?
            .fires
            .len(),
        2
    );
    assert_eq!(
        client
            .reminder_get(p::ReminderGetParams { id: reminder })
            .await?
            .fires[0]
            .state,
        p::ReminderFireState::Snoozed
    );
    client.close().await?;
    restarted.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn due_only_work_item_never_schedules_or_fires_across_time_and_restart() -> TestResult {
    let fixture = Fixture::new()?;
    let config = ServerConfig {
        scheduler_poll_interval: Duration::from_hours(1),
        ..ServerConfig::default()
    };
    let time = Arc::new(ManualTime::new("2030-01-01T00:00:00Z"));
    let handle = runtime::start_with_time(
        config.clone(),
        fixture.turso()?,
        Arc::clone(&time) as Arc<dyn attention_server::Clock>,
        Arc::clone(&time) as Arc<dyn attention_server::Sleeper>,
    )
    .await?;
    let (client, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;
    let before_create = client.snapshot_get(p::EmptyParams {}).await?.after_cursor;
    let id = work_id();
    let mut create = create_params(id.clone(), key());
    create.due_at = Some(timestamp("2030-01-02T00:00:00.000000Z"));
    client.work_item_create(create).await?;

    tokio::time::timeout(WAIT, async {
        while time.sleep_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let sleep_calls = time.sleep_calls();
    time.advance("2040-01-01T00:00:00Z");
    tokio::time::timeout(WAIT, async {
        while time.sleep_calls() == sleep_calls {
            tokio::task::yield_now().await;
        }
    })
    .await?;

    let snapshot = client.snapshot_get(p::EmptyParams {}).await?;
    assert_eq!(snapshot.state.work_items.len(), 1);
    assert_eq!(snapshot.state.work_items[0].id, id);
    assert!(snapshot.state.reminders.is_empty());
    assert!(
        snapshot
            .state
            .inbox
            .iter()
            .all(|entry| !matches!(entry, p::InboxEntryView::ReminderFire { .. }))
    );
    assert!(
        client
            .delivery_claim(p::DeliveryClaimParams {
                eligible_at: timestamp("2099-01-01T00:00:00.000000Z"),
                lease_expires_at: timestamp("2099-01-01T00:01:00.000000Z"),
                limit: 10,
            })
            .await?
            .claims
            .is_empty()
    );
    client.close().await?;
    handle.shutdown().await?;

    let restarted_time = Arc::new(ManualTime::new("2050-01-01T00:00:00Z"));
    let restarted = runtime::start_with_time(
        config,
        fixture.turso()?,
        Arc::clone(&restarted_time) as Arc<dyn attention_server::Clock>,
        Arc::clone(&restarted_time) as Arc<dyn attention_server::Sleeper>,
    )
    .await?;
    let (client, _) = Client::connect(client_config(&restarted, p::SubscriptionRequest::None))?;
    tokio::time::timeout(WAIT, async {
        while restarted_time.sleep_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let snapshot = client.snapshot_get(p::EmptyParams {}).await?;
    assert!(snapshot.state.reminders.is_empty());
    let events = match client
        .changes_get(p::ChangesGetParams {
            after_cursor: before_create,
            limit: 10,
        })
        .await?
    {
        p::ChangesResult::Page { events, .. } => events,
        p::ChangesResult::Gap { .. } => panic!("unexpected change gap"),
    };
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, p::ChangeKind::WorkItemCreated);
    client.close().await?;
    restarted.shutdown().await?;

    let database = Builder::new_local(
        fixture
            .turso()?
            .database_directory()
            .database_file()
            .to_str()
            .ok_or("database path encoding")?,
    )
    .build()
    .await?;
    let connection = database.connect()?;
    let mut rows = connection
        .query(
            "SELECT (SELECT count(*) FROM reminders), \
                    (SELECT count(*) FROM reminder_fires), \
                    (SELECT count(*) FROM outbox_intents)",
            (),
        )
        .await?;
    let row = rows.next().await?.ok_or("due-only inventory")?;
    assert_eq!(
        (row.get::<i64>(0)?, row.get::<i64>(1)?, row.get::<i64>(2)?),
        (0, 0, 0)
    );
    Ok(())
}

#[tokio::test]
async fn scheduler_fires_due_once_leaves_future_and_survives_restart() -> TestResult {
    let fixture = Fixture::new()?;
    let config = ServerConfig {
        scheduler_poll_interval: Duration::from_millis(10),
        ..ServerConfig::default()
    };
    let handle = fixture.start_with(config.clone()).await?;
    let (client, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;
    let work = work_id();
    client
        .work_item_create(create_params(work.clone(), key()))
        .await?;
    // A retained semantic ChangeEvent is not send authority without its own Outbox row.
    assert!(
        client
            .delivery_claim(p::DeliveryClaimParams {
                eligible_at: timestamp("2030-01-01T00:00:00.000000Z"),
                lease_expires_at: timestamp("2030-01-01T00:01:00.000000Z"),
                limit: 10,
            })
            .await?
            .claims
            .is_empty()
    );
    let future_work = work_id();
    client
        .work_item_create(create_params(future_work.clone(), key()))
        .await?;
    let due_id = p::ReminderId(k::ReminderId::new().to_string());
    let due_fire = p::ReminderFireId(k::ReminderFireId::new().to_string());
    let future_id = p::ReminderId(k::ReminderId::new().to_string());
    let future_fire = p::ReminderFireId(k::ReminderFireId::new().to_string());
    let before_fire = client.snapshot_get(p::EmptyParams {}).await?.after_cursor;
    for (reminder_id, fire_id, target_work, trigger_at) in [
        (
            due_id.clone(),
            due_fire.clone(),
            work,
            "2020-01-01T00:00:00.000000Z",
        ),
        (
            future_id.clone(),
            future_fire,
            future_work,
            "2099-01-01T00:00:00.000000Z",
        ),
    ] {
        client
            .reminder_create(p::CreateReminderParams {
                reminder_id,
                initial_fire_id: fire_id,
                target: p::ReminderTarget::WorkItem {
                    work_item_id: target_work,
                },
                trigger_at: timestamp(trigger_at),
                idempotency_key: key(),
            })
            .await?;
    }
    tokio::time::timeout(WAIT, async {
        loop {
            let due = client
                .reminder_get(p::ReminderGetParams { id: due_id.clone() })
                .await
                .expect("due reminder");
            if due.fires[0].state == p::ReminderFireState::Fired {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    let fired_page = client
        .changes_get(p::ChangesGetParams {
            after_cursor: before_fire,
            limit: 10,
        })
        .await?;
    let fired_events = match fired_page {
        p::ChangesResult::Page { events, .. } => events
            .into_iter()
            .filter(|event| event.kind == p::ChangeKind::ReminderFired)
            .collect::<Vec<_>>(),
        p::ChangesResult::Gap { .. } => panic!("unexpected change gap"),
    };
    assert_eq!(fired_events.len(), 1);
    let fired_event = &fired_events[0];
    assert_eq!(fired_event.kind, p::ChangeKind::ReminderFired);
    assert!(matches!(
        fired_event.inbox.upserts.as_slice(),
        [p::InboxEntryView::ReminderFire { fire, .. }] if fire.id == due_fire
            && fire.state == p::ReminderFireState::Fired
    ));
    let snapshot = client.snapshot_get(p::EmptyParams {}).await?;
    assert!(snapshot.state.inbox.iter().any(|entry| matches!(
        entry,
        p::InboxEntryView::ReminderFire { fire, .. }
            if fire.id == due_fire && fire.state == p::ReminderFireState::Fired
    )));
    assert_eq!(
        client
            .reminder_get(p::ReminderGetParams {
                id: future_id.clone(),
            })
            .await?
            .fires[0]
            .state,
        p::ReminderFireState::Scheduled
    );
    let claims = client
        .delivery_claim(p::DeliveryClaimParams {
            eligible_at: timestamp("2030-01-01T00:00:00.000000Z"),
            lease_expires_at: timestamp("2030-01-01T00:01:00.000000Z"),
            limit: 10,
        })
        .await?;
    assert_eq!(claims.claims.len(), 1);
    let intent_id = claims.claims[0].intent_id.clone();
    let authority = client
        .delivery_inspect(p::DeliveryInspectParams {
            intent_id: intent_id.clone(),
        })
        .await?;
    assert!(matches!(
        authority.intent.subject,
        p::DeliverySubject::ReminderFire { ref reminder_fire_id } if reminder_fire_id == &due_fire
    ));
    client.close().await?;
    handle.shutdown().await?;

    let restarted = fixture.start_with(config).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (client, _) = Client::connect(client_config(&restarted, p::SubscriptionRequest::None))?;
    assert_eq!(
        client
            .reminder_get(p::ReminderGetParams { id: due_id })
            .await?
            .fires[0]
            .state,
        p::ReminderFireState::Fired
    );
    assert!(
        client
            .delivery_claim(p::DeliveryClaimParams {
                eligible_at: timestamp("2030-01-01T00:00:30.000000Z"),
                lease_expires_at: timestamp("2030-01-01T00:02:00.000000Z"),
                limit: 10,
            })
            .await?
            .claims
            .is_empty()
    );
    assert_eq!(
        client
            .delivery_inspect(p::DeliveryInspectParams { intent_id })
            .await?
            .intent
            .purpose,
        p::DeliveryPurpose::ReminderFired
    );
    assert_eq!(
        client
            .reminder_get(p::ReminderGetParams { id: future_id })
            .await?
            .fires[0]
            .state,
        p::ReminderFireState::Scheduled
    );
    client.close().await?;
    restarted.shutdown().await?;

    let database = Builder::new_local(
        fixture
            .turso()?
            .database_directory()
            .database_file()
            .to_str()
            .ok_or("database path encoding")?,
    )
    .build()
    .await?;
    let connection = database.connect()?;
    let mut rows = connection
        .query(
            "SELECT (SELECT count(*) FROM change_events WHERE kind = 6), \
                    (SELECT count(*) FROM outbox_intents WHERE subject_id = ?1), \
                    (SELECT count(*) FROM delivery_states d JOIN outbox_intents o ON o.id = d.intent_id WHERE o.subject_id = ?1)",
            params![due_fire.as_str()],
        )
        .await?;
    let row = rows.next().await?.ok_or("scheduler inventory")?;
    assert_eq!(
        (row.get::<i64>(0)?, row.get::<i64>(1)?, row.get::<i64>(2)?),
        (1, 1, 1)
    );
    Ok(())
}

#[tokio::test]
async fn signal_source_lifecycle_and_attention_state_remain_independent() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let (client, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;
    let entity = p::SourceEntityIdentity {
        id: p::SourceEntityId(k::SourceEntityId::new().to_string()),
        key: p::SourceEntityKey {
            source_kind: p::SourceKind("github".into()),
            source_instance: p::SourceInstance("acme".into()),
            external_entity_id: p::ExternalEntityId("signal-state".into()),
        },
    };
    let signal = p::AttentionSignalId(k::AttentionSignalId::new().to_string());
    let mut resolved = ingest(
        p::SourceReceiptId(k::SourceReceiptId::new().to_string()),
        signal.clone(),
        entity.clone(),
        "resolved-1",
        ordered("updated_at", Some("AQ")),
        key(),
    );
    resolved.source_lifecycle = p::SignalSourceLifecycle::Resolved;
    resolved.fresh_attention = false;
    let result = client.source_occurrence_ingest(resolved).await?;
    assert!(result.outbox_intent_id.is_none());
    let snapshot = client.snapshot_get(p::EmptyParams {}).await?;
    assert!(snapshot.state.inbox.iter().any(|entry| matches!(entry,
        p::InboxEntryView::AttentionSignal { attention_signal } if attention_signal.id == signal
            && attention_signal.source_lifecycle == p::SignalSourceLifecycle::Resolved
            && attention_signal.attention_state == p::SignalAttentionState::Unread)));

    let mut active = ingest(
        p::SourceReceiptId(k::SourceReceiptId::new().to_string()),
        signal.clone(),
        entity.clone(),
        "active-2",
        ordered("updated_at", Some("Ag")),
        key(),
    );
    active.fresh_attention = false;
    assert!(
        client
            .source_occurrence_ingest(active)
            .await?
            .outbox_intent_id
            .is_none()
    );
    assert!(
        client
            .attention_signal_acknowledge(p::AcknowledgeAttentionSignalParams {
                id: signal.clone(),
                expected_revision: revision("2"),
                idempotency_key: key(),
            })
            .await?
            .outbox_intent_id
            .is_none()
    );
    let acknowledged = client
        .attention_signal_get(p::AttentionSignalGetParams { id: signal.clone() })
        .await?;
    assert_eq!(
        acknowledged.source_lifecycle,
        p::SignalSourceLifecycle::Active
    );
    assert_eq!(
        acknowledged.attention_state,
        p::SignalAttentionState::Acknowledged
    );
    assert!(client.snapshot_get(p::EmptyParams {}).await?.state.inbox.iter().all(|entry|
        !matches!(entry, p::InboxEntryView::AttentionSignal { attention_signal } if attention_signal.id == signal)));

    let mut newer = ingest(
        p::SourceReceiptId(k::SourceReceiptId::new().to_string()),
        signal.clone(),
        entity.clone(),
        "resolved-3",
        ordered("updated_at", Some("Aw")),
        key(),
    );
    newer.source_lifecycle = p::SignalSourceLifecycle::Resolved;
    newer.fresh_attention = false;
    assert!(
        client
            .source_occurrence_ingest(newer)
            .await?
            .outbox_intent_id
            .is_none()
    );
    for (occurrence, order, reason) in [
        (
            "stale",
            ordered("updated_at", Some("AQ")),
            p::ReceiptOnlyReason::Older,
        ),
        (
            "equal",
            ordered("updated_at", Some("Aw")),
            p::ReceiptOnlyReason::Equal,
        ),
        (
            "incomparable",
            p::SourceOrder::Unordered,
            p::ReceiptOnlyReason::Incomparable,
        ),
    ] {
        let mut refresh = ingest(
            p::SourceReceiptId(k::SourceReceiptId::new().to_string()),
            signal.clone(),
            entity.clone(),
            occurrence,
            order,
            key(),
        );
        refresh.fresh_attention = true;
        let refresh = client.source_occurrence_ingest(refresh).await?;
        assert_eq!(
            refresh.value.decision,
            p::SourceIngestionDecision::ReceiptOnly { reason }
        );
        assert!(refresh.outbox_intent_id.is_none());
        let view = client
            .attention_signal_get(p::AttentionSignalGetParams { id: signal.clone() })
            .await?;
        assert_eq!(view.source_lifecycle, p::SignalSourceLifecycle::Resolved);
        assert_eq!(view.attention_state, p::SignalAttentionState::Acknowledged);
        assert!(client.snapshot_get(p::EmptyParams {}).await?.state.inbox.iter().all(|entry|
            !matches!(entry, p::InboxEntryView::AttentionSignal { attention_signal } if attention_signal.id == signal)));
    }
    assert!(
        client
            .delivery_claim(p::DeliveryClaimParams {
                eligible_at: timestamp("2099-01-01T00:00:00.000000Z"),
                lease_expires_at: timestamp("2099-01-01T00:01:00.000000Z"),
                limit: 10,
            })
            .await?
            .claims
            .is_empty()
    );
    client.close().await?;
    handle.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn post_commit_publication_loss_recovers_frozen_event_on_resume() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let identity = handle.identity().clone();
    let (client, mut subscription) =
        Client::connect(client_config(&handle, p::SubscriptionRequest::Snapshot))?;
    let snapshot = tokio::time::timeout(WAIT, subscription.snapshots.recv())
        .await?
        .ok_or("snapshot")?;
    client
        .acknowledge_snapshot(snapshot.after_cursor.clone())
        .await?;
    let id = work_id();
    let mutation_key = key();
    attention_server::fail_after_commit_before_publication_once(mutation_key.clone());
    assert!(matches!(
        client
            .work_item_create(create_params(id.clone(), mutation_key.clone()))
            .await,
        Err(ClientError::AmbiguousMutation)
    ));
    client.close().await?;

    let resume = p::SubscriptionRequest::Resume {
        server_id: identity.server_id,
        stream_id: identity.stream_id,
        after_cursor: snapshot.after_cursor,
    };
    let (reconnected, mut resumed) = Client::connect(client_config(&handle, resume))?;
    let recovered = next_change(&mut resumed).await;
    assert_eq!(recovered.kind, p::ChangeKind::WorkItemCreated);
    assert!(matches!(recovered.affected.as_slice(),
        [p::AffectedView::WorkItem { work_item }] if work_item.id == id));
    reconnected
        .acknowledge_cursor(recovered.cursor.clone())
        .await?;
    let replay = reconnected
        .work_item_create(create_params(id, mutation_key))
        .await?;
    assert_eq!(replay.disposition, p::MutationDisposition::Replayed);
    assert_eq!(replay.cursor, recovered.cursor);
    assert_eq!(replay.change_event_id, recovered.id);
    let history = reconnected
        .changes_get(p::ChangesGetParams {
            after_cursor: p::Cursor("1".into()),
            limit: 10,
        })
        .await?;
    let p::ChangesResult::Page { events, .. } = history else {
        panic!("change gap")
    };
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], recovered);
    reconnected.close().await?;
    handle.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn restart_while_snoozed_fires_exact_replacement_once() -> TestResult {
    let fixture = Fixture::new()?;
    let time = Arc::new(ManualTime::new("2030-01-01T00:00:00Z"));
    let config = ServerConfig {
        scheduler_poll_interval: Duration::from_hours(1),
        ..ServerConfig::default()
    };
    let start = |time: Arc<ManualTime>| {
        let clock: Arc<dyn attention_server::Clock> = Arc::clone(&time) as Arc<_>;
        let sleeper: Arc<dyn attention_server::Sleeper> = time as Arc<_>;
        runtime::start_with_time(
            config.clone(),
            fixture.turso().expect("config"),
            clock,
            sleeper,
        )
    };
    let handle = start(Arc::clone(&time)).await?;
    let (client, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;
    let work = work_id();
    client
        .work_item_create(create_params(work.clone(), key()))
        .await?;
    let reminder = p::ReminderId(k::ReminderId::new().to_string());
    let first = p::ReminderFireId(k::ReminderFireId::new().to_string());
    client
        .reminder_create(p::CreateReminderParams {
            reminder_id: reminder.clone(),
            initial_fire_id: first.clone(),
            target: p::ReminderTarget::WorkItem { work_item_id: work },
            trigger_at: timestamp("2029-01-01T00:00:00.000000Z"),
            idempotency_key: key(),
        })
        .await?;
    time.advance("2030-01-01T00:00:01Z");
    tokio::time::timeout(WAIT, async {
        loop {
            if client
                .reminder_get(p::ReminderGetParams {
                    id: reminder.clone(),
                })
                .await
                .expect("reminder")
                .fires[0]
                .state
                == p::ReminderFireState::Fired
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let replacement = p::ReminderFireId(k::ReminderFireId::new().to_string());
    let snoozed = client
        .reminder_fire_snooze(p::SnoozeReminderFireParams {
            reminder_id: reminder.clone(),
            fire_id: first,
            replacement_fire_id: replacement.clone(),
            replacement_trigger_at: timestamp("2030-01-01T00:10:00.000000Z"),
            expected_revision: revision("2"),
            idempotency_key: key(),
        })
        .await?;
    assert!(snoozed.outbox_intent_id.is_none());
    let before = snoozed.cursor;
    client.close().await?;
    handle.shutdown().await?;

    let restarted = start(Arc::clone(&time)).await?;
    let (client, _) = Client::connect(client_config(&restarted, p::SubscriptionRequest::None))?;
    let view = client
        .reminder_get(p::ReminderGetParams {
            id: reminder.clone(),
        })
        .await?;
    assert!(
        view.fires
            .iter()
            .any(|fire| fire.id == replacement && fire.state == p::ReminderFireState::Scheduled)
    );
    time.advance("2030-01-01T00:10:00Z");
    tokio::time::timeout(WAIT, async {
        loop {
            if client
                .reminder_get(p::ReminderGetParams {
                    id: reminder.clone(),
                })
                .await
                .expect("reminder")
                .fires
                .iter()
                .any(|fire| fire.id == replacement && fire.state == p::ReminderFireState::Fired)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    time.advance("2040-01-01T00:00:00Z");
    tokio::task::yield_now().await;
    let p::ChangesResult::Page { events, .. } = client
        .changes_get(p::ChangesGetParams {
            after_cursor: before,
            limit: 10,
        })
        .await?
    else {
        panic!("gap")
    };
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, p::ChangeKind::ReminderFired);
    assert!(matches!(events[0].inbox.upserts.as_slice(),
        [p::InboxEntryView::ReminderFire { fire, .. }] if fire.id == replacement));
    let claims = client
        .delivery_claim(p::DeliveryClaimParams {
            eligible_at: timestamp("2040-01-01T00:00:00.000000Z"),
            lease_expires_at: timestamp("2040-01-01T00:01:00.000000Z"),
            limit: 10,
        })
        .await?
        .claims;
    assert_eq!(claims.len(), 2); // original fire plus exactly one replacement intent
    let mut replacement_intents = 0;
    for claim in claims {
        let authority = client
            .delivery_inspect(p::DeliveryInspectParams {
                intent_id: claim.intent_id,
            })
            .await?;
        if matches!(authority.intent.subject, p::DeliverySubject::ReminderFire { reminder_fire_id } if reminder_fire_id == replacement)
        {
            replacement_intents += 1;
        }
    }
    assert_eq!(replacement_intents, 1);
    client.close().await?;
    restarted.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn manual_scheduler_snooze_acknowledge_and_cancelled_sleep() -> TestResult {
    let fixture = Fixture::new()?;
    let time = Arc::new(ManualTime::new("2030-01-01T00:00:00Z"));
    let clock: Arc<dyn attention_server::Clock> = Arc::clone(&time) as Arc<_>;
    let sleeper: Arc<dyn attention_server::Sleeper> = Arc::clone(&time) as Arc<_>;
    let handle = runtime::start_with_time(
        ServerConfig {
            scheduler_poll_interval: Duration::from_hours(1),
            ..ServerConfig::default()
        },
        fixture.turso()?,
        clock,
        sleeper,
    )
    .await?;
    let (client, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;
    let work = work_id();
    client
        .work_item_create(create_params(work.clone(), key()))
        .await?;
    let reminder = p::ReminderId(k::ReminderId::new().to_string());
    let first = p::ReminderFireId(k::ReminderFireId::new().to_string());
    client
        .reminder_create(p::CreateReminderParams {
            reminder_id: reminder.clone(),
            initial_fire_id: first.clone(),
            target: p::ReminderTarget::WorkItem { work_item_id: work },
            trigger_at: timestamp("2030-01-01T00:01:00.000000Z"),
            idempotency_key: key(),
        })
        .await?;
    assert_eq!(
        client
            .reminder_get(p::ReminderGetParams {
                id: reminder.clone()
            })
            .await?
            .fires[0]
            .state,
        p::ReminderFireState::Scheduled
    );
    // Inject one known-uncommitted scheduler pass: the first pass happened before
    // the fire became due and committed nothing. The same persisted fire identity
    // must then be used by the woken retry.
    tokio::time::timeout(WAIT, async {
        while time.sleep_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let before_due = client.snapshot_get(p::EmptyParams {}).await?.after_cursor;
    assert!(
        client
            .delivery_claim(p::DeliveryClaimParams {
                eligible_at: timestamp("2030-01-01T00:00:00.000000Z"),
                lease_expires_at: timestamp("2030-01-01T00:00:30.000000Z"),
                limit: 10,
            })
            .await?
            .claims
            .is_empty()
    );
    time.advance("2030-01-01T00:01:00Z");
    tokio::time::timeout(WAIT, async {
        loop {
            if client
                .reminder_get(p::ReminderGetParams {
                    id: reminder.clone(),
                })
                .await
                .expect("reminder")
                .fires[0]
                .state
                == p::ReminderFireState::Fired
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let first_fire_events = match client
        .changes_get(p::ChangesGetParams {
            after_cursor: before_due,
            limit: 10,
        })
        .await?
    {
        p::ChangesResult::Page { events, .. } => events,
        p::ChangesResult::Gap { .. } => panic!("unexpected change gap"),
    };
    assert_eq!(first_fire_events.len(), 1);
    assert_eq!(first_fire_events[0].kind, p::ChangeKind::ReminderFired);
    assert!(matches!(
        first_fire_events[0].inbox.upserts.as_slice(),
        [p::InboxEntryView::ReminderFire { fire, .. }] if fire.id == first
    ));
    let first_claim = client
        .delivery_claim(p::DeliveryClaimParams {
            eligible_at: timestamp("2030-01-01T00:01:00.000000Z"),
            lease_expires_at: timestamp("2030-01-01T00:01:30.000000Z"),
            limit: 10,
        })
        .await?;
    assert_eq!(first_claim.claims.len(), 1);
    let first_authority = client
        .delivery_inspect(p::DeliveryInspectParams {
            intent_id: first_claim.claims[0].intent_id.clone(),
        })
        .await?;
    assert!(matches!(
        first_authority.intent.subject,
        p::DeliverySubject::ReminderFire { reminder_fire_id } if reminder_fire_id == first
    ));
    let replacement = p::ReminderFireId(k::ReminderFireId::new().to_string());
    client
        .reminder_fire_snooze(p::SnoozeReminderFireParams {
            reminder_id: reminder.clone(),
            fire_id: first,
            replacement_fire_id: replacement.clone(),
            replacement_trigger_at: timestamp("2030-01-01T00:02:00.000000Z"),
            expected_revision: revision("2"),
            idempotency_key: key(),
        })
        .await?;
    time.advance("2030-01-01T00:02:00Z");
    tokio::time::timeout(WAIT, async {
        loop {
            let view = client
                .reminder_get(p::ReminderGetParams {
                    id: reminder.clone(),
                })
                .await
                .expect("reminder");
            if view
                .fires
                .iter()
                .any(|fire| fire.id == replacement && fire.state == p::ReminderFireState::Fired)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    client
        .reminder_fire_acknowledge(p::AcknowledgeReminderFireParams {
            reminder_id: reminder.clone(),
            fire_id: replacement,
            expected_revision: revision("4"),
            idempotency_key: key(),
        })
        .await?;
    time.advance("2040-01-01T00:00:00Z");
    tokio::task::yield_now().await;
    let view = client
        .reminder_get(p::ReminderGetParams { id: reminder })
        .await?;
    assert_eq!(view.fires.len(), 2);
    assert_eq!(view.fires[1].state, p::ReminderFireState::Acknowledged);
    client.close().await?;
    // The scheduler is asleep for an hour, but cancellation-selectable sleep makes shutdown prompt.
    tokio::time::timeout(WAIT, handle.shutdown()).await??;
    let reopened = fixture.start().await?;
    reopened.shutdown().await?;
    Ok(())
}

async fn one_claim(
    client: &Client,
    eligible: &str,
    expires: &str,
) -> TestResult<p::DeliveryClaimCapability> {
    Ok(tokio::time::timeout(WAIT, async {
        loop {
            let claims = client
                .delivery_claim(p::DeliveryClaimParams {
                    eligible_at: timestamp(eligible),
                    lease_expires_at: timestamp(expires),
                    limit: 1,
                })
                .await
                .expect("claim")
                .claims;
            if let Some(claim) = claims.into_iter().next() {
                break claim;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?)
}

async fn seed_delivery(client: &Client) -> TestResult {
    let work = work_id();
    client
        .work_item_create(create_params(work.clone(), key()))
        .await?;
    client
        .reminder_create(p::CreateReminderParams {
            reminder_id: p::ReminderId(k::ReminderId::new().to_string()),
            initial_fire_id: p::ReminderFireId(k::ReminderFireId::new().to_string()),
            target: p::ReminderTarget::WorkItem { work_item_id: work },
            trigger_at: timestamp("2020-01-01T00:00:00.000000Z"),
            idempotency_key: key(),
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn generic_worker_restart_boundaries_prove_suppression_and_duplicate_window() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture
        .start_with(ServerConfig {
            scheduler_poll_interval: Duration::from_millis(10),
            ..ServerConfig::default()
        })
        .await?;
    let (client, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;
    seed_delivery(&client).await?;
    let sent = one_claim(
        &client,
        "2030-01-01T00:00:00.000000Z",
        "2030-01-01T00:01:00.000000Z",
    )
    .await?;
    assert_eq!(
        client
            .delivery_succeed(p::DeliverySucceedParams {
                intent_id: sent.intent_id.clone(),
                lease_token: sent.lease_token,
                provider_message_id: p::ProviderMessageId("provider-success".into()),
                succeeded_at: timestamp("2030-01-01T00:00:10.000000Z"),
            })
            .await?
            .outcome,
        p::DeliveryCompletionOutcome::Applied
    );
    // Simulate losing the external worker wakeup/checkpoint after durable success.
    client.close().await?;
    handle.shutdown().await?;
    let restarted = fixture
        .start_with(ServerConfig {
            bind: "127.0.0.1:0".parse()?,
            scheduler_poll_interval: Duration::from_millis(10),
            ..ServerConfig::default()
        })
        .await?;
    let (worker, _) = Client::connect(client_config(&restarted, p::SubscriptionRequest::None))?;
    assert!(
        matches!(worker.delivery_inspect(p::DeliveryInspectParams { intent_id: sent.intent_id.clone() }).await?.state,
        p::DeliveryStateView::Succeeded { ref provider_message_id, .. } if provider_message_id.as_str() == "provider-success")
    );
    let candidates = worker
        .delivery_claim(p::DeliveryClaimParams {
            eligible_at: timestamp("2040-01-01T00:00:00.000000Z"),
            lease_expires_at: timestamp("2040-01-01T00:01:00.000000Z"),
            limit: 16,
        })
        .await?
        .claims;
    assert!(
        candidates
            .iter()
            .all(|claim| claim.intent_id != sent.intent_id)
    );
    seed_delivery(&worker).await?;
    let ambiguous = one_claim(
        &worker,
        "2050-01-01T00:00:00.000000Z",
        "2050-01-01T00:01:00.000000Z",
    )
    .await?;
    // The provider accepted this claim, but no durable success state was committed.
    worker.close().await?;
    restarted.shutdown().await?;
    let again = fixture
        .start_with(ServerConfig {
            bind: "127.0.0.1:0".parse()?,
            scheduler_poll_interval: Duration::from_millis(10),
            ..ServerConfig::default()
        })
        .await?;
    let (replacement, _) = Client::connect(client_config(&again, p::SubscriptionRequest::None))?;
    let reclaimed = one_claim(
        &replacement,
        "2050-01-01T00:02:00.000000Z",
        "2050-01-01T00:03:00.000000Z",
    )
    .await?;
    assert_eq!(reclaimed.intent_id, ambiguous.intent_id);
    assert_ne!(reclaimed.lease_token, ambiguous.lease_token);
    replacement.close().await?;
    again.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn worker_lifecycle_fencing_concurrency_and_no_semantic_events() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture
        .start_with(ServerConfig {
            scheduler_poll_interval: Duration::from_millis(10),
            ..ServerConfig::default()
        })
        .await?;
    let (client, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;

    // Create six durable Outbox authorities through semantic reminder fires.
    for _ in 0..6 {
        let work = work_id();
        client
            .work_item_create(create_params(work.clone(), key()))
            .await?;
        client
            .reminder_create(p::CreateReminderParams {
                reminder_id: p::ReminderId(k::ReminderId::new().to_string()),
                initial_fire_id: p::ReminderFireId(k::ReminderFireId::new().to_string()),
                target: p::ReminderTarget::WorkItem { work_item_id: work },
                trigger_at: timestamp("2020-01-01T00:00:00.000000Z"),
                idempotency_key: key(),
            })
            .await?;
    }
    let claims = tokio::time::timeout(WAIT, async {
        loop {
            let claims = client
                .delivery_claim(p::DeliveryClaimParams {
                    eligible_at: timestamp("2030-01-01T00:00:00.000000Z"),
                    lease_expires_at: timestamp("2030-01-01T00:01:00.000000Z"),
                    limit: 4,
                })
                .await
                .expect("claim");
            if claims.claims.len() == 4 {
                break claims.claims;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;

    // Independent public clients atomically claim disjoint remaining authorities.
    let (client2, _) = Client::connect(client_config(&handle, p::SubscriptionRequest::None))?;
    let concurrent_params = p::DeliveryClaimParams {
        eligible_at: timestamp("2030-01-01T00:00:00.000000Z"),
        lease_expires_at: timestamp("2030-01-01T00:01:00.000000Z"),
        limit: 1,
    };
    let (left, right) = tokio::join!(
        client.delivery_claim(concurrent_params.clone()),
        client2.delivery_claim(concurrent_params)
    );
    let left = left?.claims.into_iter().next().ok_or("left claim")?;
    let right = right?.claims.into_iter().next().ok_or("right claim")?;
    assert_ne!(left.intent_id, right.intent_id);
    let before = client.snapshot_get(p::EmptyParams {}).await?.after_cursor;
    client2.close().await?;

    let first = &claims[0];
    assert_eq!(
        client
            .delivery_renew(p::DeliveryRenewParams {
                intent_id: first.intent_id.clone(),
                lease_token: first.lease_token.clone(),
                expires_at: timestamp("2030-01-01T00:02:00.000000Z"),
            })
            .await?
            .outcome,
        p::DeliveryRenewalOutcome::Renewed
    );
    let success = p::DeliverySucceedParams {
        intent_id: first.intent_id.clone(),
        lease_token: first.lease_token.clone(),
        provider_message_id: p::ProviderMessageId("provider-1".into()),
        succeeded_at: timestamp("2030-01-01T00:00:10.000000Z"),
    };
    assert_eq!(
        client.delivery_succeed(success.clone()).await?.outcome,
        p::DeliveryCompletionOutcome::Applied
    );
    assert_eq!(
        client.delivery_succeed(success.clone()).await?.outcome,
        p::DeliveryCompletionOutcome::Repeated
    );
    let mut changed_success = success;
    changed_success.provider_message_id = p::ProviderMessageId("provider-2".into());
    assert_eq!(
        client.delivery_succeed(changed_success).await?.outcome,
        p::DeliveryCompletionOutcome::Conflict
    );
    assert!(matches!(
        client
            .delivery_inspect(p::DeliveryInspectParams {
                intent_id: first.intent_id.clone(),
            })
            .await?
            .state,
        p::DeliveryStateView::Succeeded {
            ref provider_message_id,
            ..
        } if provider_message_id.as_str() == "provider-1"
    ));

    let retry = &claims[1];
    let retry_params = p::DeliveryFailRetryableParams {
        intent_id: retry.intent_id.clone(),
        lease_token: retry.lease_token.clone(),
        attempt: 1,
        error: "temporary".into(),
        next_retry_at: timestamp("2030-01-01T00:03:00.000000Z"),
    };
    assert_eq!(
        client
            .delivery_fail_retryable(retry_params.clone())
            .await?
            .outcome,
        p::DeliveryCompletionOutcome::Applied
    );
    assert_eq!(
        client
            .delivery_fail_retryable(retry_params.clone())
            .await?
            .outcome,
        p::DeliveryCompletionOutcome::Repeated
    );
    let mut changed_retry = retry_params;
    changed_retry.error = "different".into();
    assert_eq!(
        client.delivery_fail_retryable(changed_retry).await?.outcome,
        p::DeliveryCompletionOutcome::Conflict
    );
    assert!(matches!(
        client
            .delivery_inspect(p::DeliveryInspectParams {
                intent_id: retry.intent_id.clone()
            })
            .await?
            .state,
        p::DeliveryStateView::Retryable { attempt: 1, .. }
    ));

    let terminal = &claims[2];
    let terminal_params = p::DeliveryFailTerminalParams {
        intent_id: terminal.intent_id.clone(),
        lease_token: terminal.lease_token.clone(),
        attempt: 3,
        error: "permanent".into(),
        failed_at: timestamp("2030-01-01T00:00:20.000000Z"),
    };
    assert_eq!(
        client
            .delivery_fail_terminal(terminal_params.clone())
            .await?
            .outcome,
        p::DeliveryCompletionOutcome::Applied
    );
    assert_eq!(
        client
            .delivery_fail_terminal(terminal_params.clone())
            .await?
            .outcome,
        p::DeliveryCompletionOutcome::Repeated
    );
    let mut changed_terminal = terminal_params;
    changed_terminal.error = "different".into();
    assert_eq!(
        client
            .delivery_fail_terminal(changed_terminal)
            .await?
            .outcome,
        p::DeliveryCompletionOutcome::Conflict
    );

    // Expired lease is reclaimed with a fresh bearer capability; stale bearer is fenced.
    let stale = &claims[3];
    let reclaimed = client
        .delivery_claim(p::DeliveryClaimParams {
            eligible_at: timestamp("2030-01-01T00:02:00.000000Z"),
            lease_expires_at: timestamp("2030-01-01T00:04:00.000000Z"),
            limit: 1,
        })
        .await?
        .claims
        .into_iter()
        .next()
        .ok_or("reclaimed lease")?;
    assert_eq!(reclaimed.intent_id, stale.intent_id);
    assert_ne!(reclaimed.lease_token, stale.lease_token);
    assert_eq!(
        client
            .delivery_renew(p::DeliveryRenewParams {
                intent_id: stale.intent_id.clone(),
                lease_token: stale.lease_token.clone(),
                expires_at: timestamp("2030-01-01T00:05:00.000000Z"),
            })
            .await?
            .outcome,
        p::DeliveryRenewalOutcome::Fenced
    );
    assert_eq!(
        client
            .delivery_succeed(p::DeliverySucceedParams {
                intent_id: stale.intent_id.clone(),
                lease_token: stale.lease_token.clone(),
                provider_message_id: p::ProviderMessageId("stale".into()),
                succeeded_at: timestamp("2030-01-01T00:02:10.000000Z"),
            })
            .await?
            .outcome,
        p::DeliveryCompletionOutcome::Fenced
    );
    assert_eq!(
        client
            .delivery_fail_retryable(p::DeliveryFailRetryableParams {
                intent_id: stale.intent_id.clone(),
                lease_token: stale.lease_token.clone(),
                attempt: 2,
                error: "stale".into(),
                next_retry_at: timestamp("2030-01-01T00:06:00.000000Z"),
            })
            .await?
            .outcome,
        p::DeliveryCompletionOutcome::Fenced
    );
    assert_eq!(
        client
            .delivery_fail_terminal(p::DeliveryFailTerminalParams {
                intent_id: stale.intent_id.clone(),
                lease_token: stale.lease_token.clone(),
                attempt: 2,
                error: "stale".into(),
                failed_at: timestamp("2030-01-01T00:02:10.000000Z"),
            })
            .await?
            .outcome,
        p::DeliveryCompletionOutcome::Fenced
    );

    // Delivery bookkeeping never appends semantic ChangeEvents.
    assert_eq!(
        client.snapshot_get(p::EmptyParams {}).await?.after_cursor,
        before
    );
    let page = client
        .changes_get(p::ChangesGetParams {
            after_cursor: before,
            limit: 16,
        })
        .await?;
    assert!(matches!(page, p::ChangesResult::Page { ref events, .. } if events.is_empty()));

    client.close().await?;
    handle.shutdown().await?;
    Ok(())
}

async fn raw(address: std::net::SocketAddr) -> TestResult<Ws> {
    Ok(connect_async(format!("ws://{address}/v1/ws")).await?.0)
}
async fn hello(ws: &mut Ws) -> TestResult {
    ws.send(Message::Text(
        json!({
            "jsonrpc":"2.0", "id":"hello", "method":"rpc.hello",
            "params":{"protocol_version":1, "subscription":{"mode":"none"}}
        })
        .to_string()
        .into(),
    ))
    .await?;
    let response = tokio::time::timeout(WAIT, ws.next())
        .await?
        .ok_or("hello closed")??;
    let value: Value = serde_json::from_str(response.to_text()?)?;
    if value.get("result").is_none() {
        return Err(format!("hello failed: {value}").into());
    }
    Ok(())
}
async fn raw_call(ws: &mut Ws, id: &str, method: &str, params: Value) -> TestResult<Value> {
    ws.send(Message::Text(
        json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
            .to_string()
            .into(),
    ))
    .await?;
    loop {
        let message = tokio::time::timeout(WAIT, ws.next())
            .await?
            .ok_or("raw closed")??;
        let value: Value = serde_json::from_str(message.to_text()?)?;
        if value.get("id") == Some(&json!(id)) || value.get("id") == Some(&Value::Null) {
            return Ok(value);
        }
    }
}

#[tokio::test]
async fn bounds_and_secret_rejection_have_zero_persistence() -> TestResult {
    let logs = LogCapture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(logs.clone())
        .finish();
    let _logging = tracing::subscriber::set_default(subscriber);
    let fixture = Fixture::new()?;
    let config = ServerConfig {
        max_message_bytes: 512,
        max_json_depth: 8,
        max_json_nodes: 40,
        max_source_component_bytes: 8,
        max_delivery_claims: 2,
        max_delivery_text_bytes: 8,
        ..ServerConfig::default()
    };
    let handle = runtime::start(config, fixture.turso()?).await?;
    let mut ws = raw(handle.address()).await?;
    hello(&mut ws).await?;
    let before = raw_call(&mut ws, "before", "attention.snapshot.get", json!({})).await?;
    let before_cursor = before["result"]["after_cursor"].clone();
    let id = work_id();
    let base = serde_json::to_value(create_params(id, key()))?;
    for (name, params) in [
        ("secret", {
            let mut v = base.clone();
            v.as_object_mut()
                .expect("object")
                .insert("api_key".into(), json!("CANARY-do-not-log"));
            v
        }),
        ("source", {
            let mut v = base.clone();
            v.as_object_mut().expect("object").insert("source_link".into(), json!({"source_kind":"too-long-kind","source_instance":"x","external_entity_id":"y"}));
            v
        }),
        ("external-entity", {
            let mut v = base.clone();
            v.as_object_mut().expect("object").insert("source_link".into(), json!({"source_kind":"x","source_instance":"x","external_entity_id":"too-long-entity"}));
            v
        }),
        (
            "occurrence",
            json!({
                "receipt_id": k::SourceReceiptId::new().to_string(),
                "signal_id": k::AttentionSignalId::new().to_string(),
                "occurrence_key":{"source_kind":"x","source_instance":"x","occurrence_id":"too-long-occurrence"},
                "occurred_at":"2026-08-13T10:00:00.000000Z", "order":{"mode":"unordered"},
                "source_lifecycle":"active", "fresh_attention":false,
                "idempotency_key":k::MutationIdempotencyKey::new().to_string()
            }),
        ),
        ("nodes", json!({"junk": (0..50).collect::<Vec<_>>() })),
        (
            "depth",
            json!({"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":{"i":1}}}}}}}}}),
        ),
    ] {
        let method = if name == "occurrence" {
            "attention.source_occurrence.ingest"
        } else {
            "attention.work_item.create"
        };
        let response = raw_call(&mut ws, name, method, params).await?;
        assert!(response.get("error").is_some(), "{name}: {response}");
        assert!(!response.to_string().contains("CANARY-do-not-log"));
    }
    let after = raw_call(&mut ws, "after", "attention.snapshot.get", json!({})).await?;
    assert_eq!(after["result"]["after_cursor"], before_cursor);
    assert!(
        after["result"]["state"]["work_items"]
            .as_array()
            .expect("items")
            .is_empty()
    );
    assert!(!logs.text().contains("CANARY-do-not-log"));

    // Worker-specific bounds and security: the sole legitimate lease_token field
    // passes the scanner, while malformed capabilities and bounded text fail.
    let intent = k::OutboxIntentId::new().to_string();
    let token_canary = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";
    for (id, method, params, expect_result) in [
        (
            "lease-token-allowed",
            "attention.delivery.renew",
            json!({"intent_id":intent,"lease_token":token_canary,"expires_at":"2030-01-01T00:00:00.000000Z"}),
            true,
        ),
        (
            "bad-token",
            "attention.delivery.renew",
            json!({"intent_id":intent,"lease_token":"AQ","expires_at":"2030-01-01T00:00:00.000000Z"}),
            false,
        ),
        (
            "zero-claim",
            "attention.delivery.claim",
            json!({"eligible_at":"2030-01-01T00:00:00.000000Z","lease_expires_at":"2030-01-01T00:01:00.000000Z","limit":0}),
            false,
        ),
        (
            "large-claim",
            "attention.delivery.claim",
            json!({"eligible_at":"2030-01-01T00:00:00.000000Z","lease_expires_at":"2030-01-01T00:01:00.000000Z","limit":3}),
            false,
        ),
        (
            "large-provider",
            "attention.delivery.succeed",
            json!({"intent_id":intent,"lease_token":token_canary,"provider_message_id":"123456789","succeeded_at":"2030-01-01T00:00:00.000000Z"}),
            false,
        ),
        (
            "large-failure",
            "attention.delivery.fail_terminal",
            json!({"intent_id":intent,"lease_token":token_canary,"attempt":1,"error":"CANARY-FAILURE-NOT-LOGGED","failed_at":"2030-01-01T00:00:00.000000Z"}),
            false,
        ),
    ] {
        let response = raw_call(&mut ws, id, method, params).await?;
        assert_eq!(
            response.get("result").is_some(),
            expect_result,
            "{id}: {response}"
        );
    }
    assert!(!logs.text().contains(token_canary));
    assert!(!logs.text().contains("CANARY-FAILURE-NOT-LOGGED"));
    ws.send(Message::Text("x".repeat(513).into())).await?;
    let oversized = tokio::time::timeout(WAIT, ws.next())
        .await?
        .ok_or("oversized connection ended without rejection")?;
    match oversized {
        Ok(Message::Close(Some(frame))) => assert_eq!(u16::from(frame.code), 1009),
        Err(tokio_tungstenite::tungstenite::Error::Protocol(
            tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
        )) => {}
        other => panic!("unexpected oversized-message result: {other:?}"),
    }
    handle.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn real_snapshot_resume_future_gap_shutdown_clients_and_immediate_reopen() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let identity = handle.identity().clone();
    let (client, mut subscription) =
        Client::connect(client_config(&handle, p::SubscriptionRequest::Snapshot))?;
    let snapshot = tokio::time::timeout(WAIT, subscription.snapshots.recv())
        .await?
        .ok_or("snapshot")?;
    client
        .acknowledge_snapshot(snapshot.after_cursor.clone())
        .await?;
    let created = client
        .work_item_create(create_params(work_id(), key()))
        .await?;
    let live = next_change(&mut subscription).await;
    assert_eq!(live.cursor, created.cursor);
    client.acknowledge_cursor(live.cursor.clone()).await?;
    client.close().await?;

    let resume = p::SubscriptionRequest::Resume {
        server_id: identity.server_id.clone(),
        stream_id: identity.stream_id.clone(),
        after_cursor: snapshot.after_cursor,
    };
    let (resumed, mut resumed_sub) = Client::connect(client_config(&handle, resume))?;
    assert_eq!(next_change(&mut resumed_sub).await.cursor, created.cursor);
    resumed.close().await?;
    let future = p::SubscriptionRequest::Resume {
        server_id: identity.server_id,
        stream_id: identity.stream_id,
        after_cursor: p::Cursor("999999".into()),
    };
    let (future_client, mut future_sub) = Client::connect(client_config(&handle, future))?;
    let issue = tokio::time::timeout(WAIT, future_sub.issues.recv())
        .await?
        .ok_or("future gap")?;
    assert_eq!(peer_code(issue.error), p::CURSOR_GAP);
    future_client.close().await?;

    let mut active = raw(handle.address()).await?;
    hello(&mut active).await?;
    let mut prehello = raw(handle.address()).await?;

    // Hold a real SQLite writer lock so a service mutation remains in-flight.
    // Shutdown must close both connection phases but cannot finish/close the DB
    // until that request is released and drained.
    let config = fixture.turso()?;
    let path = config.database_directory().database_file();
    let blocker = Builder::new_local(path.to_str().ok_or("database path encoding")?)
        .build()
        .await?;
    let blocker_connection = blocker.connect()?;
    blocker_connection.execute("BEGIN IMMEDIATE", ()).await?;
    active
        .send(Message::Text(
            json!({
                "jsonrpc":"2.0", "id":"blocked", "method":"attention.work_item.create",
                "params":create_params(work_id(), key())
            })
            .to_string()
            .into(),
        ))
        .await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let mut shutdown = tokio::spawn(handle.shutdown());
    let active_close = tokio::time::timeout(WAIT, active.next())
        .await?
        .ok_or("active connection ended without close")??;
    let prehello_close = tokio::time::timeout(WAIT, prehello.next())
        .await?
        .ok_or("prehello connection ended without close")??;
    assert!(matches!(active_close, Message::Close(Some(frame)) if u16::from(frame.code) == 1001));
    assert!(matches!(prehello_close, Message::Close(Some(frame)) if u16::from(frame.code) == 1001));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut shutdown)
            .await
            .is_err()
    );
    blocker_connection.execute("ROLLBACK", ()).await?;
    drop(blocker_connection);
    drop(blocker);
    shutdown.await??;

    // Retention-floor mutation is the narrow test hook: production intentionally
    // has no public API for advancing retention yet.
    let database = Builder::new_local(path.to_str().ok_or("database path encoding")?)
        .build()
        .await?;
    let connection = database.connect()?;
    connection
        .execute(
            "UPDATE attention_stream_state SET floor_cursor = ?1 WHERE singleton = 1",
            params![2_u64.to_be_bytes().to_vec()],
        )
        .await?;
    drop(connection);
    drop(database);

    let reopened = fixture.start().await?;
    let expired = p::SubscriptionRequest::Resume {
        server_id: reopened.identity().server_id.clone(),
        stream_id: reopened.identity().stream_id.clone(),
        after_cursor: p::Cursor("1".into()),
    };
    let (expired_client, mut expired_sub) = Client::connect(client_config(&reopened, expired))?;
    let issue = tokio::time::timeout(WAIT, expired_sub.issues.recv())
        .await?
        .ok_or("expired gap")?;
    assert_eq!(peer_code(issue.error), p::CURSOR_GAP);
    expired_client.close().await?;
    reopened.shutdown().await?;
    Ok(())
}
