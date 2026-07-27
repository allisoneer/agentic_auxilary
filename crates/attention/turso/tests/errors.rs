use attention_turso::Error;
use std::error::Error as StdError;
use std::io;
use turso_db::Builder;
use turso_db::params;
use turso_db::transaction::TransactionBehavior;

type TestResult<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;

#[test]
fn exact_tag_error_variants_map_without_message_parsing() {
    assert!(matches!(
        Error::from(turso_db::Error::Busy(String::new())),
        Error::Busy(_)
    ));
    assert!(matches!(
        Error::from(turso_db::Error::BusySnapshot(String::new())),
        Error::BusySnapshot(_)
    ));
    assert!(matches!(
        Error::from(turso_db::Error::Constraint(String::new())),
        Error::Constraint(_)
    ));
    assert!(matches!(
        Error::from(turso_db::Error::Readonly(String::new())),
        Error::ReadOnly(_)
    ));
    assert!(matches!(
        Error::from(turso_db::Error::DatabaseFull(String::new())),
        Error::Full(_)
    ));
    assert!(matches!(
        Error::from(turso_db::Error::NotAdb(String::new())),
        Error::Corrupt(_)
    ));
    assert!(matches!(
        Error::from(turso_db::Error::Corrupt(String::new())),
        Error::Corrupt(_)
    ));
    assert!(matches!(
        Error::from(turso_db::Error::IoError(
            io::ErrorKind::PermissionDenied,
            "fixture"
        )),
        Error::Io(_)
    ));
}

#[tokio::test]
async fn real_constraint_busy_and_busy_snapshot_are_typed() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = root.path().join("errors.db");
    let path = path.to_str().ok_or("database path is not UTF-8")?;
    let database = Builder::new_local(path).build().await?;
    let setup = database.connect()?;
    setup
        .execute("CREATE TABLE errors_probe (id INTEGER PRIMARY KEY)", ())
        .await?;
    setup
        .execute("INSERT INTO errors_probe (id) VALUES (?1)", params![1_i64])
        .await?;
    let constraint = setup
        .execute("INSERT INTO errors_probe (id) VALUES (?1)", params![1_i64])
        .await
        .map_err(Error::from);
    assert!(matches!(constraint, Err(Error::Constraint(_))));

    let mut first = database.connect()?;
    let mut second = database.connect()?;
    let first_write = first
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    let busy = second
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(Error::from);
    assert!(matches!(busy, Err(Error::Busy(_))));
    first_write.rollback().await?;

    let mut stale = database.connect()?;
    let mut current = database.connect()?;
    let stale_snapshot = stale
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await?;
    let mut rows = stale_snapshot
        .query("SELECT count(*) FROM errors_probe", ())
        .await?;
    rows.next().await?;
    drop(rows);
    let current_write = current
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    current_write
        .execute("INSERT INTO errors_probe (id) VALUES (?1)", params![2_i64])
        .await?;
    current_write.commit().await?;
    let stale_write = stale_snapshot
        .execute("INSERT INTO errors_probe (id) VALUES (?1)", params![3_i64])
        .await
        .map_err(Error::from);
    assert!(matches!(stale_write, Err(Error::BusySnapshot(_))));
    Ok(())
}
