use attention_turso::WAL_OPERATIONAL_LIMIT_BYTES;
use std::error::Error;
use std::fs;
use std::path::Path;
use turso_db::Builder;
use turso_db::Database;
use turso_db::params;
use turso_db::transaction::TransactionBehavior;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

#[tokio::test]
async fn sustained_writes_with_long_reader_have_bounded_complete_inventory() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = root.path().join("wal.db");
    let database = open_database(&path).await?;
    let setup = database.connect()?;
    setup
        .execute(
            "CREATE TABLE wal_probe (id INTEGER PRIMARY KEY, payload BLOB NOT NULL)",
            (),
        )
        .await?;
    setup
        .execute(
            "INSERT INTO wal_probe (id, payload) VALUES (?1, ?2)",
            params![0_i64, vec![0_u8; 4096]],
        )
        .await?;
    drop(setup);

    let mut reader = database.connect()?;
    let snapshot = reader
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await?;
    let mut rows = snapshot.query("SELECT count(*) FROM wal_probe", ()).await?;
    let row = rows.next().await?.ok_or("snapshot returned no row")?;
    assert_eq!(row.get::<i64>(0)?, 1);
    drop(rows);

    let mut writer = database.connect()?;
    let mut maximum_bytes = 0;
    for id in 1_i64..=512 {
        let transaction = writer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await?;
        transaction
            .execute(
                "INSERT INTO wal_probe (id, payload) VALUES (?1, ?2)",
                params![id, vec![id as u8; 4096]],
            )
            .await?;
        transaction.commit().await?;
        if id % 64 == 0 {
            for (_, size) in regular_file_inventory(root.path())? {
                maximum_bytes = maximum_bytes.max(size);
            }
        }
    }
    writer.cacheflush()?;
    let inventory = regular_file_inventory(root.path())?;
    let names: Vec<_> = inventory.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["wal.db", "wal.db-wal"]);
    assert!(inventory.iter().any(|(name, _)| name == "wal.db"));
    assert!(inventory.iter().all(|(name, _)| {
        fs::symlink_metadata(root.path().join(name))
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    }));
    assert!(maximum_bytes <= WAL_OPERATIONAL_LIMIT_BYTES);
    eprintln!(
        "exact-tag WAL measurement: max_regular_file_bytes={maximum_bytes}, threshold_bytes={WAL_OPERATIONAL_LIMIT_BYTES}, inventory={inventory:?}"
    );

    snapshot.rollback().await?;
    drop(reader);
    drop(writer);
    drop(database);
    for _ in 0..3 {
        let reopened = open_database(&path).await?;
        let connection = reopened.connect()?;
        let mut rows = connection
            .query("SELECT count(*) FROM wal_probe", ())
            .await?;
        let row = rows.next().await?.ok_or("reopen count returned no row")?;
        assert_eq!(row.get::<i64>(0)?, 513);
    }
    Ok(())
}

async fn open_database(path: &Path) -> TestResult<Database> {
    let path = path
        .to_str()
        .ok_or("temporary database path is not UTF-8")?;
    Ok(Builder::new_local(path).build().await?)
}

fn regular_file_inventory(root: &Path) -> TestResult<Vec<(String, u64)>> {
    let mut inventory = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            inventory.push((
                entry.file_name().to_string_lossy().into_owned(),
                entry.metadata()?.len(),
            ));
        }
    }
    inventory.sort_unstable();
    Ok(inventory)
}
