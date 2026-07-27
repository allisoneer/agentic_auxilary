mod support;

use attention_turso::AttentionDatabase;
use attention_turso::Config;
use attention_turso::ProbeResolution;
use attention_turso::ProbeWriteOutcome;
use std::error::Error as StdError;
use std::future::Future;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::task::Poll;
use std::time::Duration;
use support::pause_at;
use support::wait_for_file;
use turso_db::Builder;
use turso_db::params;
use turso_db::transaction::TransactionBehavior;

type TestResult<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;

const OPERATION_ID: &str = "crash-operation";
const FINGERPRINT: &[u8] = b"stable-fingerprint";
const VALUE: &[u8] = b"stable-value";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crash_boundaries_resolve_without_blind_replay() -> TestResult {
    for boundary in [
        "before-effects",
        "before-commit",
        "commit-window",
        "after-commit",
    ] {
        let root = tempfile::tempdir()?;
        let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
        let database = AttentionDatabase::open(config.clone()).await?;
        database.run_startup_migrations().await?;
        database.close().await?;

        let barrier = root.path().join("crash-barrier");
        let executable = std::env::current_exe()?;
        let mut child = Command::new(executable)
            .arg("--exact")
            .arg("crash_worker")
            .arg("--nocapture")
            .env("ATTENTION_TURSO_CRASH_CHILD", boundary)
            .env(
                "ATTENTION_TURSO_CRASH_DB",
                config.database_directory().database_file(),
            )
            .env("ATTENTION_TURSO_CRASH_BARRIER", &barrier)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        wait_for_file(&barrier, Duration::from_secs(10))?;
        child.kill()?;
        let status = child.wait()?;
        assert!(
            !status.success(),
            "killed crash child unexpectedly succeeded"
        );

        let database = AttentionDatabase::open(config).await?;
        let resolution = database
            .resolve_qualification_probe(OPERATION_ID, FINGERPRINT)
            .await?;
        match boundary {
            "before-effects" | "before-commit" => {
                assert_eq!(resolution, ProbeResolution::DefinitelyAbsent);
            }
            "after-commit" => {
                assert_eq!(resolution, ProbeResolution::Matching(VALUE.to_vec()));
            }
            "commit-window" => match resolution {
                ProbeResolution::DefinitelyAbsent => {
                    assert_eq!(
                        database
                            .write_qualification_probe(OPERATION_ID, FINGERPRINT, VALUE)
                            .await?,
                        ProbeWriteOutcome::Applied
                    );
                }
                ProbeResolution::Matching(value) => assert_eq!(value, VALUE),
                ProbeResolution::IdentityConflict => {
                    return Err("commit window resolved to identity conflict".into());
                }
            },
            _ => return Err("unknown crash boundary".into()),
        }
        database.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn crash_worker() -> TestResult {
    let Ok(boundary) = std::env::var("ATTENTION_TURSO_CRASH_CHILD") else {
        return Ok(());
    };
    let database_path = PathBuf::from(
        std::env::var_os("ATTENTION_TURSO_CRASH_DB")
            .ok_or("ATTENTION_TURSO_CRASH_DB is required in child mode")?,
    );
    let barrier = PathBuf::from(
        std::env::var_os("ATTENTION_TURSO_CRASH_BARRIER")
            .ok_or("ATTENTION_TURSO_CRASH_BARRIER is required in child mode")?,
    );
    let path = database_path.to_str().ok_or("database path is not UTF-8")?;
    let database = Builder::new_local(path).build().await?;
    let mut connection = database.connect()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    if boundary == "before-effects" {
        pause_at(&barrier).await?;
    }
    transaction
        .execute(
            "INSERT INTO __attention_probe (operation_id, fingerprint, value) VALUES (?1, ?2, ?3)",
            params![OPERATION_ID, FINGERPRINT.to_vec(), VALUE.to_vec()],
        )
        .await?;
    if boundary == "before-commit" {
        pause_at(&barrier).await?;
    }
    if boundary == "commit-window" {
        let mut commit = Box::pin(transaction.commit());
        let first_poll =
            std::future::poll_fn(|context| Poll::Ready(commit.as_mut().poll(context))).await;
        if let Poll::Ready(result) = first_poll {
            result?;
        }
        pause_at(&barrier).await?;
        return Ok(());
    }
    transaction.commit().await?;
    pause_at(&barrier).await?;
    Ok(())
}
