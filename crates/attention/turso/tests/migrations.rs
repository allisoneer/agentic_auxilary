mod support;

use attention_kernel::AttentionReadPort;
use attention_kernel::ReminderFireId;
use attention_kernel::ReminderFireState;
use attention_kernel::ReminderId;
use attention_kernel::WorkItemId;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use attention_turso::Error;
use sha2::Digest;
use sha2::Sha256;
use std::error::Error as StdError;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;
use support::pause_at;
use support::wait_for_file;
use turso_db::Builder;
use turso_db::params;
use turso_db::transaction::TransactionBehavior;

type TestResult<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;

const MIGRATION_SQL: &str = include_str!("../migrations/0001_foundation.sql");
const CORE_MIGRATION_SQL: &str = include_str!("../migrations/0002_attention_core.sql");
const DELIVERY_MIGRATION_SQL: &str = include_str!("../migrations/0003_durable_delivery.sql");
const DUPLICATE_CURRENT_FIRE_REPAIR_SETUP_SQL: &str =
    include_str!("../maintenance/repair_duplicate_current_reminder_fires_setup.sql");
const DUPLICATE_CURRENT_FIRE_REPAIR_APPLY_SQL: &str =
    include_str!("../maintenance/repair_duplicate_current_reminder_fires_apply.sql");

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReminderFixtureSnapshot {
    revision: Vec<u8>,
    current_fire_id: Option<String>,
    fires: Vec<FireFixtureSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FireFixtureSnapshot {
    id: String,
    reminder_id: String,
    ordinal: i64,
    trigger_at: String,
    state: i64,
}

fn config(root: &Path) -> Result<Config, attention_turso::PathError> {
    Config::new(root.join("database"), root.join("backups"))
}

fn checksum() -> Vec<u8> {
    Sha256::digest(MIGRATION_SQL.as_bytes()).to_vec()
}

fn core_checksum() -> Vec<u8> {
    Sha256::digest(CORE_MIGRATION_SQL.as_bytes()).to_vec()
}

async fn install_head_two(connection: &turso_db::Connection) -> TestResult {
    connection.execute_batch(MIGRATION_SQL).await?;
    connection.execute_batch(CORE_MIGRATION_SQL).await?;
    connection
        .execute(
            "INSERT INTO __attention_migrations (version, name, checksum) VALUES
             (?1, ?2, ?3), (?4, ?5, ?6)",
            params![
                1_i64,
                "foundation",
                checksum(),
                2_i64,
                "attention_core",
                core_checksum()
            ],
        )
        .await?;
    Ok(())
}

async fn reminder_fixture_snapshot(
    connection: &turso_db::Connection,
    reminder_id: &str,
) -> TestResult<ReminderFixtureSnapshot> {
    let mut reminder_rows = connection
        .query(
            "SELECT revision, current_fire_id FROM reminders WHERE id = ?1",
            params![reminder_id],
        )
        .await?;
    let reminder = reminder_rows
        .next()
        .await?
        .ok_or("reminder fixture is missing")?;
    let revision = reminder.get::<Vec<u8>>(0)?;
    let current_fire_id = reminder.get::<Option<String>>(1)?;
    drop(reminder_rows);

    let mut fire_rows = connection
        .query(
            "SELECT id, reminder_id, ordinal, trigger_at, state
             FROM reminder_fires WHERE reminder_id = ?1 ORDER BY ordinal, id",
            params![reminder_id],
        )
        .await?;
    let mut fires = Vec::new();
    while let Some(row) = fire_rows.next().await? {
        fires.push(FireFixtureSnapshot {
            id: row.get(0)?,
            reminder_id: row.get(1)?,
            ordinal: row.get(2)?,
            trigger_at: row.get(3)?,
            state: row.get(4)?,
        });
    }
    Ok(ReminderFixtureSnapshot {
        revision,
        current_fire_id,
        fires,
    })
}

#[tokio::test]
async fn empty_to_head_rerun_reopen_and_checksum_drift_refusal() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = config(root.path())?;
    let database = AttentionDatabase::open(config.clone()).await?;
    let first = database.run_startup_migrations().await?;
    assert_eq!(first.applied(), 5);
    assert_eq!(first.head(), 5);
    assert_eq!(database.run_startup_migrations().await?.applied(), 0);
    database
        .write_qualification_probe("migration", b"fingerprint", b"value")
        .await?;
    database.close().await?;
    database.reopen().await?;
    assert_eq!(database.run_startup_migrations().await?.applied(), 0);
    assert!(
        database
            .read_qualification_probe("migration")
            .await?
            .is_some()
    );
    database.close().await?;
    let raw = Builder::new_local(
        config
            .database_directory()
            .database_file()
            .to_str()
            .ok_or("database path is not UTF-8")?,
    )
    .build()
    .await?;
    let connection = raw.connect()?;
    let mut rows = connection
        .query(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name IN (\
             'attention_stream_state', 'work_items', 'source_receipts', 'source_entities', \
             'attention_signals', 'reminders', 'reminder_fires', 'mutation_outcomes', \
              'change_events', 'outbox_intents', 'delivery_states', 'delivery_checkpoints')",
            (),
        )
        .await?;
    assert_eq!(
        rows.next()
            .await?
            .ok_or("schema count missing")?
            .get::<i64>(0)?,
        12
    );
    drop(rows);
    drop(connection);
    drop(raw);

    let path = config.database_directory().database_file();
    let path = path.to_str().ok_or("database path is not UTF-8")?;
    let raw = Builder::new_local(path).build().await?;
    let connection = raw.connect()?;
    connection
        .execute(
            "UPDATE __attention_migrations SET checksum = ?1 WHERE version = 1",
            params![vec![0_u8; 32]],
        )
        .await?;
    drop(connection);
    drop(raw);
    assert!(matches!(
        AttentionDatabase::open(config).await,
        Err(Error::MigrationIntegrity(_))
    ));
    Ok(())
}

#[tokio::test]
async fn malformed_duplicate_unknown_and_too_new_ledgers_are_refused() -> TestResult {
    for rows in [
        vec![(1_i64, "foundation"), (1_i64, "foundation")],
        vec![(1_i64, "unknown")],
        vec![(2_i64, "future")],
        vec![(4_i64, "future")],
    ] {
        let root = tempfile::tempdir()?;
        let config = config(root.path())?;
        let path = config.database_directory().database_file();
        let path = path.to_str().ok_or("database path is not UTF-8")?;
        let raw = Builder::new_local(path).build().await?;
        let connection = raw.connect()?;
        connection
            .execute(
                "CREATE TABLE __attention_migrations (version INTEGER, name TEXT, checksum BLOB)",
                (),
            )
            .await?;
        for (version, name) in rows {
            connection
                .execute(
                    "INSERT INTO __attention_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
                    params![version, name, checksum()],
                )
                .await?;
        }
        drop(connection);
        drop(raw);
        assert!(matches!(
            AttentionDatabase::open(config).await,
            Err(Error::MigrationIntegrity(_))
        ));
    }
    Ok(())
}

#[tokio::test]
async fn released_head_one_fixture_upgrades_to_head_five() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = config(root.path())?;
    let path = config.database_directory().database_file();
    let path = path.to_str().ok_or("database path is not UTF-8")?;
    let raw = Builder::new_local(path).build().await?;
    let connection = raw.connect()?;
    connection.execute_batch(MIGRATION_SQL).await?;
    connection
        .execute(
            "INSERT INTO __attention_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
            params![1_i64, "foundation", checksum()],
        )
        .await?;
    drop(connection);
    drop(raw);

    let database = AttentionDatabase::open(config).await?;
    let report = database.run_startup_migrations().await?;
    assert_eq!(report.applied(), 4);
    assert_eq!(report.head(), 5);
    assert_eq!(database.run_startup_migrations().await?.applied(), 0);
    database.close().await?;
    Ok(())
}

#[tokio::test]
async fn released_head_two_fixture_backfills_pending_exactly_once() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = config(root.path())?;
    let path = config.database_directory().database_file();
    let path = path.to_str().ok_or("database path is not UTF-8")?;
    let raw = Builder::new_local(path).build().await?;
    let connection = raw.connect()?;
    install_head_two(&connection).await?;
    for index in 1_u64..=2 {
        connection
            .execute(
                "INSERT INTO change_events
                 (cursor, event_id, occurred_at, kind, payload_version, payload_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    (index + 1).to_be_bytes().to_vec(),
                    format!("event-{index}"),
                    "2026-08-04T00:00:00Z",
                    0_i64,
                    1_i64,
                    vec![u8::try_from(index)?]
                ],
            )
            .await?;
    }
    for index in 1..=2 {
        connection
            .execute(
                "INSERT INTO outbox_intents
                 (id, deduplication_key, subject_kind, subject_id, originating_event_id, created_at, purpose)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    format!("intent-{index}"),
                    format!("dedupe-{index}"),
                    0_i64,
                    format!("signal-{index}"),
                    if index == 1 { "event-1" } else { "event-2" },
                    "2026-08-04T00:00:00Z",
                    0_i64
                ],
            )
            .await?;
    }
    drop(connection);
    drop(raw);

    let database = AttentionDatabase::open(config.clone()).await?;
    let report = database.run_startup_migrations().await?;
    assert_eq!(report.applied(), 3);
    assert_eq!(report.head(), 5);
    assert_eq!(database.run_startup_migrations().await?.applied(), 0);
    database.close().await?;

    let raw = Builder::new_local(path).build().await?;
    let connection = raw.connect()?;
    let mut rows = connection
        .query(
            "SELECT intent_id, status FROM delivery_states ORDER BY intent_id",
            (),
        )
        .await?;
    let mut observed = Vec::new();
    while let Some(row) = rows.next().await? {
        observed.push((row.get::<String>(0)?, row.get::<i64>(1)?));
    }
    assert_eq!(
        observed,
        [("intent-1".to_string(), 0), ("intent-2".to_string(), 0)]
    );
    Ok(())
}

#[tokio::test]
async fn invalid_head_two_current_fire_state_rolls_back_delivery_migration() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = config(root.path())?;
    let path = config.database_directory().database_file();
    let path = path.to_str().ok_or("database path is not UTF-8")?;
    let raw = Builder::new_local(path).build().await?;
    let connection = raw.connect()?;
    install_head_two(&connection).await?;
    connection
        .execute(
            "INSERT INTO reminders
             (id, revision, target_kind, target_id, trigger_at, current_fire_id)
             VALUES (?1, ?2, 0, ?3, ?4, NULL)",
            params![
                "reminder-1",
                1_u64.to_be_bytes().to_vec(),
                "target-1",
                "2026-08-04T00:00:00Z"
            ],
        )
        .await?;
    for (id, ordinal, state) in [("fire-1", 0_i64, 0_i64), ("fire-2", 1, 1)] {
        connection
            .execute(
                "INSERT INTO reminder_fires
                 (id, reminder_id, ordinal, trigger_at, state)
                 VALUES (?1, 'reminder-1', ?2, ?3, ?4)",
                params![id, ordinal, "2026-08-04T00:00:00Z", state],
            )
            .await?;
    }
    drop(connection);
    drop(raw);

    let database = AttentionDatabase::open(config).await?;
    for _ in 0..2 {
        assert!(database.run_startup_migrations().await.is_err());
    }
    database.close().await?;

    let raw = Builder::new_local(path).build().await?;
    let connection = raw.connect()?;
    let mut rows = connection
        .query(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = 'delivery_states'",
            (),
        )
        .await?;
    assert_eq!(
        rows.next()
            .await?
            .ok_or("schema count missing")?
            .get::<i64>(0)?,
        0
    );
    let mut rows = connection
        .query(
            "SELECT count(*) FROM __attention_migrations WHERE version = 3",
            (),
        )
        .await?;
    assert_eq!(
        rows.next()
            .await?
            .ok_or("ledger count missing")?
            .get::<i64>(0)?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn duplicate_current_reminder_fire_repair_is_atomic_and_unblocks_migration_0003() -> TestResult
{
    let root = tempfile::tempdir()?;
    let config = config(root.path())?;
    let database_path = config.database_directory().database_file();
    let path = database_path.to_str().ok_or("database path is not UTF-8")?;
    let raw = Builder::new_local(path).build().await?;
    let mut connection = raw.connect()?;
    install_head_two(&connection).await?;

    let reminder_id = ReminderId::new();
    let reminder_id_text = reminder_id.to_string();
    let target_id = WorkItemId::new().to_string();
    let historical_fire_id = ReminderFireId::new();
    let historical_fire_id_text = historical_fire_id.to_string();
    let authoritative_fire_id = ReminderFireId::new();
    let authoritative_fire_id_text = authoritative_fire_id.to_string();
    let retired_fire_id = ReminderFireId::new();
    let retired_fire_id_text = retired_fire_id.to_string();
    let acknowledged_retired_fire_id = ReminderFireId::new();
    let acknowledged_retired_fire_id_text = acknowledged_retired_fire_id.to_string();
    let revision = 7_u64;

    connection
        .execute(
            "INSERT INTO reminders
             (id, revision, target_kind, target_id, trigger_at, current_fire_id)
             VALUES (?1, ?2, 0, ?3, ?4, NULL)",
            params![
                reminder_id_text.as_str(),
                revision.to_be_bytes().to_vec(),
                target_id.as_str(),
                "2026-08-04T00:00:00Z"
            ],
        )
        .await?;
    for (id, ordinal, trigger_at, state) in [
        (
            historical_fire_id_text.as_str(),
            0_i64,
            "2026-08-03T00:00:00Z",
            2_i64,
        ),
        (
            authoritative_fire_id_text.as_str(),
            1_i64,
            "2026-08-04T00:00:00Z",
            1_i64,
        ),
        (
            retired_fire_id_text.as_str(),
            2_i64,
            "2026-08-05T00:00:00Z",
            0_i64,
        ),
        (
            acknowledged_retired_fire_id_text.as_str(),
            3_i64,
            "2026-08-06T00:00:00Z",
            1_i64,
        ),
    ] {
        connection
            .execute(
                "INSERT INTO reminder_fires
                 (id, reminder_id, ordinal, trigger_at, state)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, reminder_id_text.as_str(), ordinal, trigger_at, state],
            )
            .await?;
    }
    connection
        .execute(
            "UPDATE reminders SET current_fire_id = ?1 WHERE id = ?2",
            params![
                authoritative_fire_id_text.as_str(),
                reminder_id_text.as_str()
            ],
        )
        .await?;
    let before = reminder_fixture_snapshot(&connection, &reminder_id_text).await?;

    let invalid_repair = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    invalid_repair
        .execute_batch(DUPLICATE_CURRENT_FIRE_REPAIR_SETUP_SQL)
        .await?;
    let mut inspection = invalid_repair
        .query(
            "SELECT
                 (SELECT count(*) FROM temp.__attention_repair_duplicate_reminders),
                 (SELECT count(*) FROM temp.__attention_repair_affected_fire_history)",
            (),
        )
        .await?;
    let inspection_row = inspection
        .next()
        .await?
        .ok_or("repair inspection counts are missing")?;
    assert_eq!(inspection_row.get::<i64>(0)?, 1);
    assert_eq!(inspection_row.get::<i64>(1)?, 4);
    drop(inspection);
    invalid_repair
        .execute(
            "INSERT INTO temp.__attention_repair_authoritative
             (reminder_id, authoritative_fire_id) VALUES (?1, ?2)",
            params![
                reminder_id_text.as_str(),
                authoritative_fire_id_text.as_str()
            ],
        )
        .await?;
    let apply_error = invalid_repair
        .execute_batch(DUPLICATE_CURRENT_FIRE_REPAIR_APPLY_SQL)
        .await
        .expect_err("incomplete retirement decisions must fail");
    let diagnostic = apply_error.to_string();
    for stored_id in [
        &reminder_id_text,
        &historical_fire_id_text,
        &authoritative_fire_id_text,
        &retired_fire_id_text,
        &acknowledged_retired_fire_id_text,
    ] {
        assert!(!diagnostic.contains(stored_id));
    }
    invalid_repair.rollback().await?;
    assert_eq!(
        reminder_fixture_snapshot(&connection, &reminder_id_text).await?,
        before
    );
    drop(connection);
    drop(raw);

    let database = AttentionDatabase::open(config.clone()).await?;
    assert!(database.run_startup_migrations().await.is_err());
    database.close().await?;

    let raw = Builder::new_local(path).build().await?;
    let mut connection = raw.connect()?;
    let mut migration_state = connection
        .query(
            "SELECT
                 (SELECT count(*) FROM sqlite_schema
                  WHERE type = 'table' AND name = 'delivery_states'),
                 (SELECT count(*) FROM __attention_migrations WHERE version = 3)",
            (),
        )
        .await?;
    let migration_state_row = migration_state
        .next()
        .await?
        .ok_or("migration rollback state is missing")?;
    assert_eq!(migration_state_row.get::<i64>(0)?, 0);
    assert_eq!(migration_state_row.get::<i64>(1)?, 0);
    drop(migration_state);

    let valid_repair = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    valid_repair
        .execute_batch(DUPLICATE_CURRENT_FIRE_REPAIR_SETUP_SQL)
        .await?;
    valid_repair
        .execute(
            "INSERT INTO temp.__attention_repair_authoritative
             (reminder_id, authoritative_fire_id) VALUES (?1, ?2)",
            params![
                reminder_id_text.as_str(),
                authoritative_fire_id_text.as_str()
            ],
        )
        .await?;
    valid_repair
        .execute(
            "INSERT INTO temp.__attention_repair_retire
             (reminder_id, retired_fire_id, terminal_state) VALUES (?1, ?2, ?3)",
            params![
                reminder_id_text.as_str(),
                acknowledged_retired_fire_id_text.as_str(),
                2_i64
            ],
        )
        .await?;
    valid_repair
        .execute(
            "INSERT INTO temp.__attention_repair_retire
             (reminder_id, retired_fire_id, terminal_state) VALUES (?1, ?2, ?3)",
            params![
                reminder_id_text.as_str(),
                retired_fire_id_text.as_str(),
                3_i64
            ],
        )
        .await?;
    valid_repair
        .execute_batch(DUPLICATE_CURRENT_FIRE_REPAIR_APPLY_SQL)
        .await?;
    valid_repair.commit().await?;

    let after = reminder_fixture_snapshot(&connection, &reminder_id_text).await?;
    assert_eq!(after.revision, before.revision);
    assert_eq!(
        after.current_fire_id,
        Some(authoritative_fire_id_text.clone())
    );
    assert_eq!(after.fires.len(), before.fires.len());
    for (before_fire, after_fire) in before.fires.iter().zip(&after.fires) {
        assert_eq!(after_fire.id, before_fire.id);
        assert_eq!(after_fire.reminder_id, before_fire.reminder_id);
        assert_eq!(after_fire.ordinal, before_fire.ordinal);
        assert_eq!(after_fire.trigger_at, before_fire.trigger_at);
    }
    assert_eq!(after.fires[0].state, 2);
    assert_eq!(after.fires[1].state, 1);
    assert_eq!(after.fires[2].state, 3);
    assert_eq!(after.fires[3].state, 2);
    assert_eq!(
        after
            .fires
            .iter()
            .filter(|fire| matches!(fire.state, 0 | 1))
            .count(),
        1
    );
    drop(connection);
    drop(raw);

    let database = AttentionDatabase::open(config).await?;
    let report = database.run_startup_migrations().await?;
    assert_eq!(report.applied(), 3);
    assert_eq!(report.head(), 5);
    assert_eq!(database.run_startup_migrations().await?.applied(), 0);
    let reminder = database
        .reminder(reminder_id)
        .await?
        .ok_or("repaired reminder is missing")?;
    assert_eq!(reminder.revision().value(), revision);
    assert_eq!(reminder.fires().len(), 4);
    for (fire_id, expected_state) in [
        (historical_fire_id, ReminderFireState::Acknowledged),
        (authoritative_fire_id, ReminderFireState::Fired),
        (retired_fire_id, ReminderFireState::Snoozed),
        (
            acknowledged_retired_fire_id,
            ReminderFireState::Acknowledged,
        ),
    ] {
        let fire = reminder
            .fires()
            .iter()
            .find(|fire| fire.id() == fire_id)
            .ok_or("retained repaired fire is missing")?;
        assert_eq!(fire.state(), expected_state);
    }
    database.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn child_process_interruption_never_splits_ddl_and_ledger() -> TestResult {
    for boundary in ["after-body", "before-commit", "after-commit"] {
        let root = tempfile::tempdir()?;
        let config = config(root.path())?;
        let barrier = root.path().join("barrier");
        let executable = std::env::current_exe()?;
        let mut child = Command::new(executable)
            .arg("--exact")
            .arg("child_migration_worker")
            .arg("--nocapture")
            .env("ATTENTION_TURSO_MIGRATION_CHILD", boundary)
            .env(
                "ATTENTION_TURSO_MIGRATION_DB",
                config.database_directory().database_file(),
            )
            .env("ATTENTION_TURSO_MIGRATION_BARRIER", &barrier)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        wait_for_file(&barrier, Duration::from_secs(10))?;
        child.kill()?;
        let status = child.wait()?;
        assert!(
            !status.success(),
            "killed migration child unexpectedly succeeded"
        );

        let database = AttentionDatabase::open(config).await?;
        let report = database.run_startup_migrations().await?;
        let expected = if boundary == "after-commit" { 4 } else { 5 };
        assert_eq!(report.applied(), expected, "boundary {boundary}");
        assert_eq!(database.run_startup_migrations().await?.applied(), 0);
        database.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn child_migration_worker() -> TestResult {
    let Ok(boundary) = std::env::var("ATTENTION_TURSO_MIGRATION_CHILD") else {
        return Ok(());
    };
    let database_path = PathBuf::from(
        std::env::var_os("ATTENTION_TURSO_MIGRATION_DB")
            .ok_or("ATTENTION_TURSO_MIGRATION_DB is required in child mode")?,
    );
    let barrier = PathBuf::from(
        std::env::var_os("ATTENTION_TURSO_MIGRATION_BARRIER")
            .ok_or("ATTENTION_TURSO_MIGRATION_BARRIER is required in child mode")?,
    );
    let path = database_path.to_str().ok_or("database path is not UTF-8")?;
    let database = Builder::new_local(path).build().await?;
    let mut connection = database.connect()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    transaction.execute_batch(MIGRATION_SQL).await?;
    if boundary == "after-body" {
        pause_at(&barrier).await?;
    }
    transaction
        .execute(
            "INSERT INTO __attention_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
            params![1_i64, "foundation", checksum()],
        )
        .await?;
    if boundary == "before-commit" {
        pause_at(&barrier).await?;
    }
    transaction.commit().await?;
    pause_at(&barrier).await?;
    Ok(())
}

#[test]
fn core_migration_is_nonempty_and_foundation_bytes_remain_separate() {
    assert!(!CORE_MIGRATION_SQL.is_empty());
    assert!(!DELIVERY_MIGRATION_SQL.is_empty());
    assert!(!MIGRATION_SQL.contains("attention_stream_state"));
    assert!(!CORE_MIGRATION_SQL.contains("delivery_states"));
}
