mod support;

use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use chrono::DateTime;
use chrono::Utc;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::str::FromStr;
use std::time::Duration;
use support::pause_at;
use support::wait_for_file;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn time() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-03T12:00:00Z")
        .unwrap_or_else(|error| panic!("fixed time must parse: {error}"))
        .with_timezone(&Utc)
}

fn bundle(
    work_item: WorkItemId,
    mutation: MutationIdempotencyKey,
    event: ChangeEventId,
) -> CreateWorkItemBundle {
    evaluate_create_work_item(
        &CreateWorkItem::new(work_item, None, None, None, None, mutation),
        EvaluationContext::new(event, None, time()),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn killed_before_call_or_after_commit_resolves_to_one_stable_transaction() -> TestResult {
    for cut in ["before-call", "after-commit"] {
        let root = tempfile::tempdir()?;
        let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
        let database = AttentionDatabase::open(config.clone()).await?;
        database.run_startup_migrations().await?;
        database.close().await?;

        let work_item = WorkItemId::new();
        let mutation = MutationIdempotencyKey::new();
        let event = ChangeEventId::new();
        let barrier = root.path().join("semantic-cut");
        let executable = std::env::current_exe()?;
        let mut child = Command::new(executable)
            .arg("--exact")
            .arg("child_semantic_worker")
            .arg("--nocapture")
            .env("ATTENTION_SEMANTIC_CHILD", cut)
            .env("ATTENTION_DB_DIR", config.database_directory().as_path())
            .env("ATTENTION_BACKUP_DIR", config.backup_root().as_path())
            .env("ATTENTION_WORK_ITEM", work_item.to_string())
            .env("ATTENTION_MUTATION", mutation.to_string())
            .env("ATTENTION_EVENT", event.to_string())
            .env("ATTENTION_BARRIER", &barrier)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        wait_for_file(&barrier, Duration::from_secs(10))?;
        child.kill()?;
        let status = child.wait()?;
        assert!(!status.success());

        let database = AttentionDatabase::open(config).await?;
        let resolved = database
            .commit_create_work_item(bundle(work_item, mutation, event))
            .await?;
        let expected = if cut == "before-call" {
            CommandDisposition::Applied
        } else {
            CommandDisposition::Replayed
        };
        assert_eq!(resolved.disposition(), expected);
        let changes = database
            .changes_after(ChangesAfterQuery::new(
                CommitCursor::try_from(1)?,
                QueryLimit::try_from(8)?,
            ))
            .await?;
        assert!(matches!(changes, ChangesResult::Page(ref page) if page.events().len() == 1));
        database.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn child_semantic_worker() -> TestResult {
    let Some(cut) = std::env::var_os("ATTENTION_SEMANTIC_CHILD") else {
        return Ok(());
    };
    let config = Config::new(
        PathBuf::from(std::env::var_os("ATTENTION_DB_DIR").ok_or("database directory")?),
        PathBuf::from(std::env::var_os("ATTENTION_BACKUP_DIR").ok_or("backup directory")?),
    )?;
    let work_item = WorkItemId::from_str(&std::env::var("ATTENTION_WORK_ITEM")?)?;
    let mutation = MutationIdempotencyKey::from_str(&std::env::var("ATTENTION_MUTATION")?)?;
    let event = ChangeEventId::from_str(&std::env::var("ATTENTION_EVENT")?)?;
    let barrier = PathBuf::from(std::env::var_os("ATTENTION_BARRIER").ok_or("barrier")?);
    let database = AttentionDatabase::open(config).await?;
    if cut == "before-call" {
        pause_at(&barrier).await?;
    }
    database
        .commit_create_work_item(bundle(work_item, mutation, event))
        .await?;
    pause_at(&barrier).await?;
    Ok(())
}
