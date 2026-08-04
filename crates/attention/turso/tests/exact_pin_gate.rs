mod support;

use std::convert::TryFrom;
use support::FileDatabase;
use support::TestResult;
use turso_db::Value;
use turso_db::params;
use turso_db::transaction::TransactionBehavior;

fn counter(value: u64) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

#[tokio::test]
async fn blob_counters_preserve_full_width_order_ranges_and_paging() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let connection = fixture.database.connect()?;
    connection
        .execute(
            "CREATE TABLE counters (value BLOB PRIMARY KEY NOT NULL CHECK(length(value) = 8))",
            (),
        )
        .await?;
    let values = [
        1,
        u64::from(u8::MAX),
        u64::from(u8::MAX) + 1,
        i64::MAX as u64,
        i64::MAX as u64 + 1,
        u64::MAX,
    ];
    for value in values {
        connection
            .execute(
                "INSERT INTO counters (value) VALUES (?1)",
                params![counter(value)],
            )
            .await?;
    }

    let mut equality = connection
        .query(
            "SELECT value FROM counters WHERE value = ?1",
            params![counter(i64::MAX as u64 + 1)],
        )
        .await?;
    assert_eq!(
        equality
            .next()
            .await?
            .ok_or("BLOB equality returned no row")?
            .get::<Vec<u8>>(0)?,
        counter(i64::MAX as u64 + 1)
    );
    drop(equality);

    let mut ordered = connection
        .query("SELECT value FROM counters ORDER BY value", ())
        .await?;
    let mut observed = Vec::new();
    while let Some(row) = ordered.next().await? {
        let bytes = row.get::<Vec<u8>>(0)?;
        observed.push(u64::from_be_bytes(
            bytes.try_into().map_err(|_| "counter width")?,
        ));
    }
    assert_eq!(observed, values);
    drop(ordered);

    let mut page = connection
        .query(
            "SELECT value FROM counters WHERE value > ?1 AND value <= ?2 ORDER BY value LIMIT ?3",
            params![
                counter(u64::from(u8::MAX)),
                counter(i64::MAX as u64 + 1),
                3_i64
            ],
        )
        .await?;
    let mut observed = Vec::new();
    while let Some(row) = page.next().await? {
        let bytes = row.get::<Vec<u8>>(0)?;
        observed.push(u64::from_be_bytes(
            bytes.try_into().map_err(|_| "counter width")?,
        ));
    }
    assert_eq!(
        observed,
        [u64::from(u8::MAX) + 1, i64::MAX as u64, i64::MAX as u64 + 1]
    );
    Ok(())
}

#[tokio::test]
async fn partial_unique_indexes_are_enforced_without_becoming_a_design_dependency() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let connection = fixture.database.connect()?;
    connection
        .execute_batch(
            "CREATE TABLE fires (id INTEGER PRIMARY KEY, reminder_id INTEGER NOT NULL, current INTEGER NOT NULL);\
             CREATE UNIQUE INDEX one_current_fire ON fires(reminder_id) WHERE current = 1;",
        )
        .await?;
    connection
        .execute(
            "INSERT INTO fires (id, reminder_id, current) VALUES (?1, ?2, ?3)",
            params![1_i64, 7_i64, 0_i64],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO fires (id, reminder_id, current) VALUES (?1, ?2, ?3)",
            params![2_i64, 7_i64, 0_i64],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO fires (id, reminder_id, current) VALUES (?1, ?2, ?3)",
            params![3_i64, 7_i64, 1_i64],
        )
        .await?;
    assert!(
        connection
            .execute(
                "INSERT INTO fires (id, reminder_id, current) VALUES (?1, ?2, ?3)",
                params![4_i64, 7_i64, 1_i64],
            )
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn deferred_snapshots_and_immediate_writes_use_independent_connections() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let setup = fixture.database.connect()?;
    setup
        .execute("CREATE TABLE snapshots (id INTEGER PRIMARY KEY)", ())
        .await?;
    setup
        .execute("INSERT INTO snapshots (id) VALUES (?1)", params![1_i64])
        .await?;
    drop(setup);

    let mut reader = fixture.database.connect()?;
    let snapshot = reader
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .await?;
    let mut rows = snapshot.query("SELECT count(*) FROM snapshots", ()).await?;
    assert_eq!(
        rows.next()
            .await?
            .ok_or("snapshot row missing")?
            .get::<i64>(0)?,
        1
    );
    drop(rows);

    let mut writer = fixture.database.connect()?;
    let write = writer
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    write
        .execute("INSERT INTO snapshots (id) VALUES (?1)", params![2_i64])
        .await?;
    write.commit().await?;

    let mut rows = snapshot.query("SELECT count(*) FROM snapshots", ()).await?;
    assert_eq!(
        rows.next()
            .await?
            .ok_or("snapshot row missing")?
            .get::<i64>(0)?,
        1
    );
    drop(rows);
    snapshot.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn active_write_statement_overlap_is_busy_and_rollback_recovers() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let mut connection = fixture.database.connect()?;
    connection
        .execute("CREATE TABLE overlap (id INTEGER PRIMARY KEY)", ())
        .await?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    let mut returning = transaction
        .query("INSERT INTO overlap (id) VALUES (1), (2) RETURNING id", ())
        .await?;
    assert!(returning.next().await?.is_some());
    assert!(
        transaction
            .execute("INSERT INTO overlap (id) VALUES (?1)", params![3_i64])
            .await
            .is_err()
    );
    drop(returning);
    transaction.rollback().await?;

    connection
        .execute("INSERT INTO overlap (id) VALUES (?1)", params![4_i64])
        .await?;
    Ok(())
}

#[tokio::test]
async fn signed_integer_fallback_boundary_is_explicit() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let connection = fixture.database.connect()?;
    connection
        .execute(
            "CREATE TABLE bounded (value INTEGER NOT NULL CHECK(value BETWEEN 1 AND 9223372036854775807))",
            (),
        )
        .await?;
    connection
        .execute("INSERT INTO bounded (value) VALUES (?1)", params![i64::MAX])
        .await?;
    assert!(Value::try_from(i64::MAX as u64 + 1).is_err());
    assert!(
        connection
            .execute("INSERT INTO bounded (value) VALUES (?1)", params![0_i64])
            .await
            .is_err()
    );
    Ok(())
}
