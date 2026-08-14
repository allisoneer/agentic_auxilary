use crate::dto::ConnectionStatusDto;
use crate::dto::DesktopMessageDto;
use crate::dto::ResetReason;
use crate::mutation::AcknowledgeFireInput;
use crate::mutation::AcknowledgeSignalInput;
use crate::mutation::CreateReminderInput;
use crate::mutation::CreateWorkItemInput;
use crate::mutation::ExistingWorkItemInput;
use crate::mutation::MutationResourceDto;
use crate::mutation::ReminderTargetInput;
use crate::mutation::SnoozeFireInput;
use crate::supervisor::DesktopSupervisor;
use attention_client::Client;
use attention_client::ClientConfig;
use attention_protocol as p;
use attention_server::RuntimeHandle;
use attention_server::ServerConfig;
use attention_server::runtime;
use attention_turso::Config as TursoConfig;
use chrono::DateTime;
use chrono::Utc;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::Notify;
use turso_db::Builder;
use turso_db::params;

const WAIT: Duration = Duration::from_secs(5);
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

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
        Ok(runtime::start(ServerConfig::default(), self.turso()?).await?)
    }

    async fn set_floor(&self, floor: u64) -> TestResult {
        let config = self.turso()?;
        let path = config.database_directory().database_file();
        let database = Builder::new_local(path.to_str().ok_or("database path encoding")?)
            .build()
            .await?;
        let connection = database.connect()?;
        connection
            .execute(
                "UPDATE attention_stream_state SET floor_cursor = ?1 WHERE singleton = 1",
                params![floor.to_be_bytes().to_vec()],
            )
            .await?;
        Ok(())
    }

    async fn change_stream(&self) -> TestResult {
        let config = self.turso()?;
        let path = config.database_directory().database_file();
        let database = Builder::new_local(path.to_str().ok_or("database path encoding")?)
            .build()
            .await?;
        let connection = database.connect()?;
        connection
            .execute(
                "UPDATE attention_server_identity SET stream_id = ?1 WHERE singleton = 1",
                [uuid::Uuid::now_v7().to_string()],
            )
            .await?;
        Ok(())
    }
}

fn config(handle: &RuntimeHandle, subscription: p::SubscriptionRequest) -> ClientConfig {
    let mut config = ClientConfig::new(format!("ws://{}/v1/ws", handle.address()));
    config.subscription = subscription;
    config.request_timeout = WAIT;
    config.heartbeat_interval = Duration::from_secs(30);
    config.reconnect_min = Duration::from_millis(10);
    config.reconnect_max = Duration::from_millis(20);
    config
}

async fn wait_state(
    supervisor: &DesktopSupervisor,
    predicate: impl Fn(&crate::dto::DesktopStateDto) -> bool + Send + Sync,
) -> crate::dto::DesktopStateDto {
    tokio::time::timeout(WAIT, async {
        loop {
            let state = supervisor.state().await;
            if predicate(&state) {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("desktop state timeout")
}

fn last_snapshot_cursor(state: &crate::dto::DesktopStateDto) -> Option<String> {
    state.replay.iter().rev().find_map(|message| match message {
        DesktopMessageDto::Snapshot { after_cursor, .. } => Some(after_cursor.clone()),
        _ => None,
    })
}

fn last_change_cursor(state: &crate::dto::DesktopStateDto) -> Option<String> {
    state.replay.iter().rev().find_map(|message| match message {
        DesktopMessageDto::Change { event, .. } => Some(event.cursor.clone()),
        _ => None,
    })
}

fn create_params() -> p::CreateWorkItemParams {
    p::CreateWorkItemParams {
        id: p::WorkItemId(uuid::Uuid::now_v7().to_string()),
        due_at: None,
        scheduled_at: None,
        defer_until: None,
        source_link: None,
        idempotency_key: p::MutationIdempotencyKey(uuid::Uuid::now_v7().to_string()),
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
    fn sleep(&self, _duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.sleep_calls.fetch_add(1, Ordering::SeqCst);
        let wake = Arc::clone(&self.wake);
        Box::pin(async move { wake.notified().await })
    }
}

fn inbox_has_signal(state: &crate::dto::DesktopStateDto, id: &str) -> bool {
    state.snapshot.as_ref().is_some_and(|snapshot| {
        snapshot.inbox.iter().any(|entry| {
            matches!(entry, crate::dto::InboxEntryDto::AttentionSignal { attention_signal } if attention_signal.id == id)
        })
    })
}

fn inbox_has_fire(state: &crate::dto::DesktopStateDto, id: &str) -> bool {
    state.snapshot.as_ref().is_some_and(|snapshot| {
        snapshot.inbox.iter().any(|entry| {
            matches!(entry, crate::dto::InboxEntryDto::ReminderFire { fire, .. } if fire.id == id)
        })
    })
}

async fn wait_and_ack_change(
    supervisor: &DesktopSupervisor,
    generation: u64,
    cursor: &str,
) -> TestResult {
    wait_state(supervisor, |state| {
        last_change_cursor(state).as_deref() == Some(cursor)
    })
    .await;
    supervisor
        .acknowledge_change(generation, cursor.to_owned())
        .await?;
    Ok(())
}

fn source_ingress(signal_id: &str) -> p::IngestSourceOccurrenceParams {
    p::IngestSourceOccurrenceParams {
        receipt_id: p::SourceReceiptId(uuid::Uuid::now_v7().to_string()),
        entity: Some(p::SourceEntityIdentity {
            id: p::SourceEntityId(uuid::Uuid::now_v7().to_string()),
            key: p::SourceEntityKey {
                source_kind: p::SourceKind("github".into()),
                source_instance: p::SourceInstance("desktop-live-test".into()),
                external_entity_id: p::ExternalEntityId("issue-1110".into()),
            },
        }),
        signal_id: p::AttentionSignalId(signal_id.into()),
        occurrence_key: p::OccurrenceKey {
            source_kind: p::SourceKind("github".into()),
            source_instance: p::SourceInstance("desktop-live-test".into()),
            occurrence_id: p::OccurrenceId(uuid::Uuid::now_v7().to_string()),
        },
        occurred_at: p::WireTimestamp::parse("2030-01-01T00:00:00.000000Z").expect("timestamp"),
        order: p::SourceOrder::Unordered,
        source_lifecycle: p::SignalSourceLifecycle::Active,
        fresh_attention: true,
        idempotency_key: p::MutationIdempotencyKey(uuid::Uuid::now_v7().to_string()),
    }
}

async fn bootstrap(supervisor: &DesktopSupervisor) -> TestResult<u64> {
    let initial = wait_state(supervisor, |state| state.snapshot_after_cursor.is_some()).await;
    let generation = initial.generation;
    supervisor
        .acknowledge_snapshot(
            generation,
            initial.snapshot_after_cursor.ok_or("snapshot cursor")?,
        )
        .await?;
    Ok(generation)
}

async fn wait_fire_event(
    supervisor: &DesktopSupervisor,
    fire_id: &str,
) -> (crate::dto::DesktopStateDto, String) {
    let state = wait_state(supervisor, |state| {
        state.replay.iter().any(|message| {
            matches!(message, DesktopMessageDto::Change { event, .. } if event.inbox.upserts.iter().any(|entry| matches!(entry, crate::dto::InboxEntryDto::ReminderFire { fire, .. } if fire.id == fire_id)))
        })
    })
    .await;
    let cursor = state
        .replay
        .iter()
        .find_map(|message| match message {
            DesktopMessageDto::Change { event, .. }
                if event.inbox.upserts.iter().any(|entry| {
                    matches!(entry, crate::dto::InboxEntryDto::ReminderFire { fire, .. } if fire.id == fire_id)
                }) =>
            {
                Some(event.cursor.clone())
            }
            _ => None,
        })
        .expect("fire event cursor");
    (state, cursor)
}

#[tokio::test]
async fn live_change_dto_matches_committed_frontend_vertical_fixture() -> TestResult {
    const SAFE_LITERAL_ID: &str = "01912345-6789-7abc-8def-0123456789ab";
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let supervisor =
        DesktopSupervisor::live_for_test(config(&handle, p::SubscriptionRequest::Snapshot), None)?;
    bootstrap(&supervisor).await?;
    let (writer, _) = Client::connect(config(&handle, p::SubscriptionRequest::None))?;
    let ingested = writer
        .source_occurrence_ingest(source_ingress(SAFE_LITERAL_ID))
        .await?;
    let state = wait_state(&supervisor, |state| {
        last_change_cursor(state).as_deref() == Some(ingested.cursor.0.as_str())
    })
    .await;
    let message = state
        .replay
        .iter()
        .find(|message| matches!(message, DesktopMessageDto::Change { event, .. } if event.cursor == ingested.cursor.0))
        .ok_or("live change DTO")?;
    let mut actual = serde_json::to_value(message)?;
    let object = actual.as_object_mut().ok_or("message object")?;
    object.insert("sequence".into(), serde_json::json!(2));
    object.insert("generation".into(), serde_json::json!(1));
    let event = object
        .get_mut("event")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("event object")?;
    event.insert("id".into(), serde_json::json!("LIVE_EVENT_ID"));
    event.insert("cursor".into(), serde_json::json!("LIVE_CURSOR"));
    event.insert("occurredAt".into(), serde_json::json!("LIVE_OCCURRED_AT"));
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../src/test/fixtures/live-source-change.json"
    ))?;
    assert_eq!(
        actual, expected,
        "regenerate fixture only from this live path"
    );

    writer.close().await?;
    supervisor.close().await?;
    handle.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn live_bridge_source_signal_acknowledgement_is_event_authoritative() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let supervisor =
        DesktopSupervisor::live_for_test(config(&handle, p::SubscriptionRequest::Snapshot), None)?;
    let generation = bootstrap(&supervisor).await?;
    let (writer, _) = Client::connect(config(&handle, p::SubscriptionRequest::None))?;
    let signal_id = uuid::Uuid::now_v7().to_string();
    let ingested = writer
        .source_occurrence_ingest(source_ingress(&signal_id))
        .await?;
    wait_and_ack_change(&supervisor, generation, &ingested.cursor.0).await?;
    assert!(inbox_has_signal(&supervisor.state().await, &signal_id));

    let receipt = supervisor
        .acknowledge_signal(AcknowledgeSignalInput {
            id: signal_id.clone(),
            expected_revision: "1".into(),
        })
        .await?;
    assert!(inbox_has_signal(&supervisor.state().await, &signal_id));
    wait_state(&supervisor, |state| {
        last_change_cursor(state).as_deref() == Some(receipt.cursor.as_str())
    })
    .await;
    assert!(inbox_has_signal(&supervisor.state().await, &signal_id));
    supervisor
        .acknowledge_change(generation, receipt.cursor)
        .await?;
    assert!(!inbox_has_signal(&supervisor.state().await, &signal_id));

    writer.close().await?;
    supervisor.close().await?;
    handle.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn live_bridge_scheduler_acknowledge_and_snooze_replacement_are_event_authoritative()
-> TestResult {
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
    let (writer, _) = Client::connect(config(&handle, p::SubscriptionRequest::None))?;
    let work = writer.work_item_create(create_params()).await?.value.id.0;
    let snooze_work = writer.work_item_create(create_params()).await?.value.id.0;
    let supervisor =
        DesktopSupervisor::live_for_test(config(&handle, p::SubscriptionRequest::Snapshot), None)?;
    let generation = bootstrap(&supervisor).await?;

    let created = supervisor
        .create_reminder(CreateReminderInput {
            target: ReminderTargetInput::WorkItem {
                work_item_id: work.clone(),
            },
            trigger_at: "2030-01-01T00:01:00Z".into(),
        })
        .await?;
    let MutationResourceDto::ReminderFire {
        reminder_id,
        fire_id,
    } = created.resource
    else {
        return Err("wrong reminder receipt resource".into());
    };
    assert!(
        supervisor
            .state()
            .await
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.reminders.is_empty())
    );
    wait_and_ack_change(&supervisor, generation, &created.cursor).await?;
    while time.sleep_calls() == 0 {
        tokio::task::yield_now().await;
    }
    time.advance("2030-01-01T00:01:00Z");
    let (pending_fire, fired_cursor) = wait_fire_event(&supervisor, &fire_id).await;
    assert!(!inbox_has_fire(&pending_fire, &fire_id));
    supervisor
        .acknowledge_change(generation, fired_cursor)
        .await?;
    assert!(inbox_has_fire(&supervisor.state().await, &fire_id));
    let acknowledged = supervisor
        .acknowledge_fire(AcknowledgeFireInput {
            reminder_id,
            fire_id: fire_id.clone(),
            expected_revision: "2".into(),
        })
        .await?;
    assert!(inbox_has_fire(&supervisor.state().await, &fire_id));
    wait_and_ack_change(&supervisor, generation, &acknowledged.cursor).await?;
    assert!(!inbox_has_fire(&supervisor.state().await, &fire_id));
    time.advance("2030-01-01T00:10:00Z");
    tokio::task::yield_now().await;
    assert!(!inbox_has_fire(&supervisor.state().await, &fire_id));

    let snooze_created = supervisor
        .create_reminder(CreateReminderInput {
            target: ReminderTargetInput::WorkItem {
                work_item_id: snooze_work,
            },
            trigger_at: "2030-01-01T00:11:00Z".into(),
        })
        .await?;
    let MutationResourceDto::ReminderFire {
        reminder_id: snooze_reminder,
        fire_id: old_fire,
    } = snooze_created.resource
    else {
        return Err("wrong snooze reminder resource".into());
    };
    wait_and_ack_change(&supervisor, generation, &snooze_created.cursor).await?;
    time.advance("2030-01-01T00:11:00Z");
    let (_, old_fired_cursor) = wait_fire_event(&supervisor, &old_fire).await;
    supervisor
        .acknowledge_change(generation, old_fired_cursor)
        .await?;
    assert!(inbox_has_fire(&supervisor.state().await, &old_fire));

    let snoozed = supervisor
        .snooze_fire(SnoozeFireInput {
            reminder_id: snooze_reminder.clone(),
            fire_id: old_fire.clone(),
            expected_revision: "2".into(),
            replacement_trigger_at: "2030-01-01T00:12:00Z".into(),
        })
        .await?;
    let MutationResourceDto::ReminderFire {
        fire_id: replacement,
        ..
    } = snoozed.resource
    else {
        return Err("wrong snooze receipt resource".into());
    };
    assert!(inbox_has_fire(&supervisor.state().await, &old_fire));
    wait_and_ack_change(&supervisor, generation, &snoozed.cursor).await?;
    let scheduled = supervisor.state().await;
    assert!(!inbox_has_fire(&scheduled, &old_fire));
    assert!(!inbox_has_fire(&scheduled, &replacement));
    assert!(
        scheduled
            .snapshot
            .as_ref()
            .is_some_and(
                |snapshot| snapshot
                    .reminders
                    .iter()
                    .any(|reminder| reminder.id == snooze_reminder
                        && reminder.fires.iter().any(|fire| fire.id == replacement
                            && fire.state == p::ReminderFireState::Scheduled))
            )
    );

    time.advance("2030-01-01T00:12:00Z");
    let (replacement_pending, replacement_cursor) =
        wait_fire_event(&supervisor, &replacement).await;
    assert!(!inbox_has_fire(&replacement_pending, &replacement));
    supervisor
        .acknowledge_change(generation, replacement_cursor)
        .await?;
    assert!(inbox_has_fire(&supervisor.state().await, &replacement));
    let replacement_ack = supervisor
        .acknowledge_fire(AcknowledgeFireInput {
            reminder_id: snooze_reminder,
            fire_id: replacement.clone(),
            expected_revision: "4".into(),
        })
        .await?;
    assert!(inbox_has_fire(&supervisor.state().await, &replacement));
    wait_and_ack_change(&supervisor, generation, &replacement_ack.cursor).await?;
    assert!(!inbox_has_fire(&supervisor.state().await, &replacement));

    writer.close().await?;
    supervisor.close().await?;
    handle.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn live_bridge_create_receipt_waits_for_event_and_stale_revision_is_structured() -> TestResult
{
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let supervisor =
        DesktopSupervisor::live_for_test(config(&handle, p::SubscriptionRequest::Snapshot), None)?;
    let initial = wait_state(&supervisor, |state| state.snapshot_after_cursor.is_some()).await;
    let generation = initial.generation;
    supervisor
        .acknowledge_snapshot(
            generation,
            initial
                .snapshot_after_cursor
                .clone()
                .ok_or("snapshot cursor")?,
        )
        .await?;

    let receipt = supervisor
        .create_work_item(CreateWorkItemInput {
            due_at: Some("2026-08-14T01:02:03.123456+02:00".into()),
            scheduled_at: None,
            defer_until: None,
        })
        .await?;
    let crate::mutation::MutationResourceDto::WorkItem { id: resource_id } = receipt.resource
    else {
        return Err("wrong receipt resource".into());
    };
    // The RPC receipt itself never applies materialized state; only the delivered
    // event and its acknowledgement do so.
    assert!(
        supervisor
            .state()
            .await
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.work_items.is_empty() && snapshot.inbox.is_empty())
    );
    let changed = wait_state(&supervisor, |state| {
        state.replay.iter().any(|message| {
            matches!(message,
            DesktopMessageDto::Change { event, .. } if event.id == receipt.change_event_id)
        })
    })
    .await;
    assert!(
        changed
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.work_items.is_empty())
    );
    supervisor
        .acknowledge_change(generation, receipt.cursor)
        .await?;
    let applied = supervisor.state().await;
    let item = applied
        .snapshot
        .as_ref()
        .and_then(|snapshot| {
            snapshot
                .work_items
                .iter()
                .find(|item| item.id == resource_id)
        })
        .ok_or("materialized work item")?;
    assert_eq!(item.revision, "1");
    assert!(
        applied
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| !snapshot.inbox.is_empty())
    );

    let completed = supervisor
        .complete_work_item(ExistingWorkItemInput {
            id: resource_id.clone(),
            expected_revision: "1".into(),
        })
        .await?;
    wait_state(&supervisor, |state| {
        last_change_cursor(state).as_deref() == Some(completed.cursor.as_str())
    })
    .await;
    supervisor
        .acknowledge_change(generation, completed.cursor)
        .await?;
    let stale = supervisor
        .cancel_work_item(ExistingWorkItemInput {
            id: resource_id.clone(),
            expected_revision: "1".into(),
        })
        .await
        .expect_err("stale cancel must conflict");
    assert_eq!(stale.category, "expected_revision_conflict");
    assert_eq!(stale.resource_kind, Some("work_item"));
    assert_eq!(stale.resource_id.as_deref(), Some(resource_id.as_str()));
    assert_eq!(stale.expected_revision.as_deref(), Some("1"));
    assert_eq!(stale.actual_revision.as_deref(), Some("2"));

    supervisor.close().await?;
    handle.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn live_supervisor_bootstrap_event_ack_restart_resume_and_clean_close() -> TestResult {
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let address = handle.address();
    let supervisor =
        DesktopSupervisor::live_for_test(config(&handle, p::SubscriptionRequest::Snapshot), None)
            .map_err(|e| format!("{}: {}", e.category, e.message))?;

    let state = wait_state(&supervisor, |state| state.snapshot_after_cursor.is_some()).await;
    let generation = state.generation;
    let snapshot_cursor = last_snapshot_cursor(&state).ok_or("snapshot replay")?;
    assert_eq!(
        state.snapshot_after_cursor.as_deref(),
        Some(snapshot_cursor.as_str())
    );
    supervisor
        .acknowledge_snapshot(generation, snapshot_cursor)
        .await?;

    let mut writer_config = ClientConfig::new(format!("ws://{address}/v1/ws"));
    writer_config.subscription = p::SubscriptionRequest::None;
    let (writer, _) = Client::connect(writer_config)?;
    let committed = writer.work_item_create(create_params()).await?;
    let changed = wait_state(&supervisor, |state| last_change_cursor(state).is_some()).await;
    let cursor = last_change_cursor(&changed).ok_or("change replay")?;
    assert_eq!(cursor, committed.cursor.0);
    assert!(
        supervisor
            .acknowledge_change(generation, "not-front".into())
            .await
            .is_err()
    );
    supervisor.acknowledge_change(generation, cursor).await?;
    let before_restart_sequence = supervisor.state().await.sequence;
    writer.close().await?;

    handle.shutdown().await?;
    let restarted = runtime::start(
        ServerConfig {
            bind: address,
            ..ServerConfig::default()
        },
        fixture.turso()?,
    )
    .await?;
    assert_eq!(restarted.address(), address);
    let mut disconnected_writer_config = ClientConfig::new(format!("ws://{address}/v1/ws"));
    disconnected_writer_config.subscription = p::SubscriptionRequest::None;
    let (disconnected_writer, _) = Client::connect(disconnected_writer_config)?;
    let committed_while_disconnected = disconnected_writer
        .work_item_create(create_params())
        .await?;
    let resumed = wait_state(&supervisor, |state| {
        state.sequence > before_restart_sequence
            && matches!(state.status, ConnectionStatusDto::Connected)
            && last_change_cursor(state).as_deref()
                == Some(committed_while_disconnected.cursor.0.as_str())
    })
    .await;
    assert_eq!(resumed.generation, generation);
    supervisor
        .acknowledge_change(generation, committed_while_disconnected.cursor.0)
        .await?;
    disconnected_writer.close().await?;

    supervisor.close().await?;
    supervisor.close().await?;
    restarted.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn live_supervisor_future_expired_and_stream_change_reset_to_fresh_snapshot() -> TestResult {
    // Future resume is rejected by the server; the public client reports Gap and negotiates a snapshot.
    let fixture = Fixture::new()?;
    let handle = fixture.start().await?;
    let identity = handle.identity().clone();
    let future = p::SubscriptionRequest::Resume {
        server_id: identity.server_id.clone(),
        stream_id: identity.stream_id.clone(),
        after_cursor: p::Cursor("999999".into()),
    };
    let future_supervisor = DesktopSupervisor::live_for_test(config(&handle, future), None)
        .map_err(|e| format!("{}: {}", e.category, e.message))?;
    let future_state = wait_state(&future_supervisor, |state| {
        state.generation > 1 && state.snapshot_after_cursor.is_some()
    })
    .await;
    assert!(future_state.replay.iter().any(|message| matches!(
        message,
        DesktopMessageDto::Reset {
            reason: ResetReason::Gap,
            ..
        }
    )));
    future_supervisor.close().await?;
    handle.shutdown().await?;

    fixture.set_floor(1).await?;
    let reopened = fixture.start().await?;
    let expired = p::SubscriptionRequest::Resume {
        server_id: reopened.identity().server_id.clone(),
        stream_id: reopened.identity().stream_id.clone(),
        after_cursor: p::Cursor("0".into()),
    };
    let expired_supervisor = DesktopSupervisor::live_for_test(config(&reopened, expired), None)
        .map_err(|e| format!("{}: {}", e.category, e.message))?;
    let expired_state = wait_state(&expired_supervisor, |state| {
        state.generation > 1 && state.snapshot_after_cursor.is_some()
    })
    .await;
    assert!(expired_state.replay.iter().any(|message| matches!(
        message,
        DesktopMessageDto::Reset {
            reason: ResetReason::Gap,
            ..
        }
    )));
    expired_supervisor.close().await?;
    reopened.shutdown().await?;

    let original = fixture.start().await?;
    let stream_address = original.address();
    let stream_supervisor = DesktopSupervisor::live_for_test(
        config(&original, p::SubscriptionRequest::Snapshot),
        None,
    )?;
    let initial = wait_state(&stream_supervisor, |state| {
        state.snapshot_after_cursor.is_some()
            && matches!(state.status, ConnectionStatusDto::Connected)
    })
    .await;
    let initial_generation = initial.generation;
    let original_identity = original.identity().clone();
    let initial_cursor = initial
        .snapshot_after_cursor
        .clone()
        .ok_or("initial cursor")?;
    stream_supervisor
        .acknowledge_snapshot(initial_generation, initial_cursor)
        .await?;
    original.shutdown().await?;
    stream_supervisor
        .expect_identity_for_test(original_identity.server_id, original_identity.stream_id)
        .await;
    fixture.change_stream().await?;
    let changed = runtime::start(
        ServerConfig {
            bind: stream_address,
            ..ServerConfig::default()
        },
        fixture.turso()?,
    )
    .await?;
    let stream_state = wait_state(&stream_supervisor, |state| {
        state.generation > initial_generation && state.snapshot_after_cursor.is_some()
    })
    .await;
    assert!(stream_state.replay.iter().any(|message| matches!(
        message,
        DesktopMessageDto::Reset {
            reason: ResetReason::StreamChanged,
            ..
        }
    )));
    stream_supervisor.close().await?;
    changed.shutdown().await?;
    Ok(())
}
