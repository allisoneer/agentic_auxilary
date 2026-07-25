use attention_turso::AttentionDatabase;
use attention_turso::Config;
use attention_turso::Error;
use sha2::Digest;
use sha2::Sha256;
use std::error::Error as StdError;
use std::fs;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;
use turso_db::Builder;
use turso_db::params;
use turso_db::transaction::TransactionBehavior;

type TestResult<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;

const MIGRATION_SQL: &str = include_str!("../migrations/0001_foundation.sql");

fn config(root: &Path) -> Result<Config, attention_turso::PathError> {
    Config::new(root.join("database"), root.join("backups"))
}

fn checksum() -> Vec<u8> {
    Sha256::digest(MIGRATION_SQL.as_bytes()).to_vec()
}

#[tokio::test]
async fn empty_to_head_rerun_reopen_and_checksum_drift_refusal() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = config(root.path())?;
    let database = AttentionDatabase::open(config.clone()).await?;
    let first = database.run_startup_migrations().await?;
    assert_eq!(first.applied(), 1);
    assert_eq!(first.head(), 1);
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
        let expected = usize::from(boundary != "after-commit");
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

fn wait_for_file(path: &Path, timeout: Duration) -> TestResult {
    let started = Instant::now();
    while !path.exists() {
        if started.elapsed() >= timeout {
            return Err(format!("child did not reach barrier at {}", path.display()).into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

async fn pause_at(path: &Path) -> TestResult {
    File::create(path)?;
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if fs::metadata(path).is_err() {
            return Ok(());
        }
    }
}
