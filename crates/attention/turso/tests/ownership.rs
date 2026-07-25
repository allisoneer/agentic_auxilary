use attention_turso::AttentionDatabase;
use attention_turso::BackupRoot;
use attention_turso::Config;
use attention_turso::DatabaseDirectory;
use attention_turso::Error;
use attention_turso::LifecycleState;
use std::error::Error as StdError;

type TestResult<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn retained_public_owner_types_are_send_and_sync() {
    assert_send_sync::<AttentionDatabase>();
    assert_send_sync::<Config>();
    assert_send_sync::<DatabaseDirectory>();
    assert_send_sync::<BackupRoot>();
    assert_send_sync::<Error>();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ownership_conflict_close_reopen_and_cross_task_use() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config.clone()).await?;
    assert_eq!(database.state(), LifecycleState::Open);

    let conflict = AttentionDatabase::open(config).await;
    assert!(matches!(conflict, Err(Error::Ownership(_))));

    let cross_task = database.clone();
    tokio::spawn(async move { cross_task.state() }).await?;

    database.close().await?;
    assert_eq!(database.state(), LifecycleState::Closed);
    assert!(matches!(
        database.read_qualification_probe("closed").await,
        Err(Error::Shutdown)
    ));
    database.reopen().await?;
    assert_eq!(database.state(), LifecycleState::Open);
    database.close().await?;
    database.reopen().await?;
    database.close().await?;
    Ok(())
}
