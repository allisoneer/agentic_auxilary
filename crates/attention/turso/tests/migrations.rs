mod support;

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

#[tokio::test]
async fn empty_to_head_rerun_reopen_and_checksum_drift_refusal() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = config(root.path())?;
    let database = AttentionDatabase::open(config.clone()).await?;
    let first = database.run_startup_migrations().await?;
    assert_eq!(first.applied(), 3);
    assert_eq!(first.head(), 3);
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
async fn released_head_one_fixture_upgrades_to_head_three() -> TestResult {
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
    assert_eq!(report.applied(), 2);
    assert_eq!(report.head(), 3);
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
    assert_eq!(report.applied(), 1);
    assert_eq!(report.head(), 3);
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
    assert!(database.run_startup_migrations().await.is_err());
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
        let expected = if boundary == "after-commit" { 2 } else { 3 };
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
