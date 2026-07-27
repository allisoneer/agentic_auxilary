use attention_turso::AttentionDatabase;
use attention_turso::BackupRoot;
use attention_turso::Config;
use attention_turso::DatabaseDirectory;
use attention_turso::Error;
use attention_turso::LifecycleState;
use std::error::Error as StdError;

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::fs::symlink;

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

#[cfg(unix)]
#[tokio::test]
async fn ownership_lock_targets_retained_directory_after_path_replacement() -> TestResult {
    let root = tempfile::tempdir()?;
    let database_path = root.path().join("database");
    let renamed_path = root.path().join("renamed-database");
    let config = Config::new(&database_path, root.path().join("backups"))?;

    fs::rename(&database_path, &renamed_path)?;
    fs::create_dir_all(&database_path)?;
    fs::set_permissions(&database_path, fs::Permissions::from_mode(0o700))?;

    let database = AttentionDatabase::open(config).await?;
    let lock_name = ".attention-turso.lock";
    assert!(renamed_path.join(lock_name).is_file());
    assert!(!database_path.join(lock_name).exists());
    assert_eq!(
        fs::metadata(renamed_path.join(lock_name))?
            .permissions()
            .mode()
            & 0o077,
        0
    );
    database.close().await?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn ownership_lock_symlink_fails_without_touching_sentinel() -> TestResult {
    let root = tempfile::tempdir()?;
    let database_path = root.path().join("database");
    let config = Config::new(&database_path, root.path().join("backups"))?;
    let sentinel = root.path().join("sentinel");
    fs::write(&sentinel, b"unchanged")?;
    symlink(&sentinel, database_path.join(".attention-turso.lock"))?;

    assert!(matches!(
        AttentionDatabase::open(config).await,
        Err(Error::Ownership(_))
    ));
    assert_eq!(fs::read(&sentinel)?, b"unchanged");
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn non_regular_ownership_lock_entry_fails_closed() -> TestResult {
    let root = tempfile::tempdir()?;
    let database_path = root.path().join("database");
    let config = Config::new(&database_path, root.path().join("backups"))?;
    fs::create_dir_all(database_path.join(".attention-turso.lock"))?;

    assert!(matches!(
        AttentionDatabase::open(config).await,
        Err(Error::Ownership(_))
    ));
    Ok(())
}
