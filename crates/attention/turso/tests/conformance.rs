mod support;

use attention_turso::PINNED_TURSO_VERSION;
use std::fs;
use std::path::Path;
use support::FileDatabase;
use support::TestResult;
use support::open_database;
use support::regular_file_inventory;
use turso_db::Builder;
use turso_db::named_params;
use turso_db::params;
use turso_db::transaction::TransactionBehavior;

#[test]
fn exact_defaults_off_pin_and_registry_checksum_are_retained() -> TestResult {
    assert_eq!(PINNED_TURSO_VERSION, "0.8.0-pre.1");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("crate is not nested under the workspace root")?;
    let manifest = fs::read_to_string(workspace.join("Cargo.toml"))?;
    let dependency = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("turso-db ="))
        .ok_or("workspace Turso dependency is missing")?;
    assert!(dependency.contains("package = \"turso\""));
    assert!(dependency.contains("version = \"=0.8.0-pre.1\""));
    assert!(dependency.contains("default-features = false"));
    assert!(!dependency.contains("features = ["));

    let lock = fs::read_to_string(workspace.join("Cargo.lock"))?;
    assert!(lock.contains(
        "name = \"turso\"\nversion = \"0.8.0-pre.1\"\nsource = \
         \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \
         \"ff5ff5ff2ed1280fe40122900c3153a7a10a552470b0a27aee67dfa2312da934\""
    ));
    Ok(())
}

#[tokio::test]
async fn isolated_in_memory_database_is_supported() -> TestResult {
    let database = Builder::new_local(":memory:").build().await?;
    let connection = database.connect()?;
    connection
        .execute("CREATE TABLE memory_probe (value INTEGER NOT NULL)", ())
        .await?;
    connection
        .execute(
            "INSERT INTO memory_probe (value) VALUES (?1)",
            params![9_i64],
        )
        .await?;
    let mut rows = connection
        .query("SELECT value FROM memory_probe", ())
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or("in-memory query returned no row")?;
    assert_eq!(row.get::<i64>(0)?, 9);
    Ok(())
}

#[tokio::test]
async fn file_backed_open_independent_connections_and_reopen() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let writer = fixture.database.connect()?;
    let reader = fixture.database.connect()?;

    writer
        .execute(
            "CREATE TABLE qualification (id INTEGER PRIMARY KEY, value TEXT NOT NULL)",
            (),
        )
        .await?;
    writer
        .execute(
            "INSERT INTO qualification (id, value) VALUES (?1, ?2)",
            params![1_i64, "independent"],
        )
        .await?;

    let mut rows = reader
        .query(
            "SELECT value FROM qualification WHERE id = ?1",
            params![1_i64],
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or("reader did not observe writer row")?;
    assert_eq!(row.get::<String>(0)?, "independent");

    drop(rows);
    drop(reader);
    drop(writer);
    let database_path = fixture.path().to_owned();
    drop(fixture.database);
    let reopened = open_database(&database_path).await?;
    let connection = reopened.connect()?;
    let mut rows = connection
        .query("SELECT count(*) FROM qualification", ())
        .await?;
    let row = rows.next().await?.ok_or("reopen query returned no row")?;
    assert_eq!(row.get::<i64>(0)?, 1);
    Ok(())
}

#[tokio::test]
async fn positional_named_and_typed_values_are_supported() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let connection = fixture.database.connect()?;
    connection
        .execute(
            "CREATE TABLE values_probe (id INTEGER PRIMARY KEY, n INTEGER, r REAL, t TEXT, b BLOB)",
            (),
        )
        .await?;
    connection
        .execute(
            "INSERT INTO values_probe (id, n, r, t, b) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                1_i64,
                Option::<i64>::None,
                1.25_f64,
                "text",
                vec![0_u8, 1, 2]
            ],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO values_probe (id, n, r, t, b) VALUES (:id, :n, :r, :t, :b)",
            named_params! {
                ":id": 2_i64,
                ":n": 7_i64,
                ":r": 2.5_f64,
                ":t": "named",
                ":b": vec![3_u8, 4, 5],
            },
        )
        .await?;

    let mut rows = connection
        .query(
            "SELECT n, r, t, b FROM values_probe WHERE id = ?1",
            params![1_i64],
        )
        .await?;
    let row = rows.next().await?.ok_or("typed query returned no row")?;
    assert_eq!(row.get::<Option<i64>>(0)?, None);
    assert!((row.get::<f64>(1)? - 1.25).abs() < f64::EPSILON);
    assert_eq!(row.get::<String>(2)?, "text");
    assert_eq!(row.get::<Vec<u8>>(3)?, vec![0, 1, 2]);
    assert!(
        row.get::<i64>(2).is_err(),
        "text-to-integer mismatch must fail"
    );
    assert!(
        row.get::<String>(99).is_err(),
        "out-of-range decode must fail"
    );
    Ok(())
}

#[tokio::test]
async fn constraints_upsert_and_returning_are_supported() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let connection = fixture.database.connect()?;
    connection
        .execute(
            "CREATE TABLE upsert_probe (key TEXT PRIMARY KEY, value INTEGER NOT NULL CHECK(value >= 0))",
            (),
        )
        .await?;
    connection
        .execute(
            "INSERT INTO upsert_probe (key, value) VALUES (?1, ?2)",
            params!["key", 1_i64],
        )
        .await?;
    let duplicate = connection
        .execute(
            "INSERT INTO upsert_probe (key, value) VALUES (?1, ?2)",
            params!["key", 2_i64],
        )
        .await;
    assert!(
        duplicate.is_err(),
        "unique constraint must reject duplicate key"
    );

    let mut rows = connection
        .query(
            "INSERT INTO upsert_probe (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value RETURNING value",
            params!["key", 3_i64],
        )
        .await?;
    let row = rows.next().await?.ok_or("RETURNING produced no row")?;
    assert_eq!(row.get::<i64>(0)?, 3);
    Ok(())
}

#[tokio::test]
async fn immediate_deferred_rollback_and_sql_savepoints_are_supported() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let mut connection = fixture.database.connect()?;
    connection
        .execute("CREATE TABLE tx_probe (id INTEGER PRIMARY KEY)", ())
        .await?;

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    transaction
        .execute("INSERT INTO tx_probe (id) VALUES (?1)", params![1_i64])
        .await?;
    transaction.rollback().await?;

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await?;
    let mut rows = transaction
        .query("SELECT count(*) FROM tx_probe", ())
        .await?;
    let row = rows.next().await?.ok_or("snapshot count returned no row")?;
    assert_eq!(row.get::<i64>(0)?, 0);
    drop(rows);
    transaction.commit().await?;

    connection.execute("BEGIN IMMEDIATE", ()).await?;
    connection.execute("SAVEPOINT qualification", ()).await?;
    connection
        .execute("INSERT INTO tx_probe (id) VALUES (?1)", params![2_i64])
        .await?;
    connection.execute("ROLLBACK TO qualification", ()).await?;
    connection.execute("RELEASE qualification", ()).await?;
    connection.execute("COMMIT", ()).await?;

    let mut rows = connection
        .query("SELECT count(*) FROM tx_probe", ())
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or("post-savepoint count returned no row")?;
    assert_eq!(row.get::<i64>(0)?, 0);
    Ok(())
}

#[tokio::test]
async fn real_write_contention_is_reported() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let setup = fixture.database.connect()?;
    setup
        .execute("CREATE TABLE contention_probe (id INTEGER PRIMARY KEY)", ())
        .await?;
    drop(setup);

    let mut first = fixture.database.connect()?;
    let mut second = fixture.database.connect()?;
    let first_transaction = first
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    first_transaction
        .execute(
            "INSERT INTO contention_probe (id) VALUES (?1)",
            params![1_i64],
        )
        .await?;
    let second_result = second
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await;
    assert!(
        second_result.is_err(),
        "second immediate writer must encounter contention"
    );
    first_transaction.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn cacheflush_and_file_creation_are_observations_not_checkpoint_claims() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let connection = fixture.database.connect()?;
    connection
        .execute("CREATE TABLE file_probe (value BLOB NOT NULL)", ())
        .await?;
    connection
        .execute(
            "INSERT INTO file_probe (value) VALUES (?1)",
            params![vec![7_u8; 4096]],
        )
        .await?;
    connection.cacheflush()?;

    let root = fixture.path().parent().ok_or("database has no parent")?;
    let inventory = regular_file_inventory(root)?;
    assert!(
        !inventory.is_empty(),
        "local database must create regular files"
    );
    assert!(
        fs::metadata(fixture.path()).is_ok(),
        "nominal database file must exist"
    );
    eprintln!("exact-tag cacheflush file observation: {inventory:?}");
    Ok(())
}
