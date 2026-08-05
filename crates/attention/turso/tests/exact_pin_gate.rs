mod support;

use std::convert::TryFrom;
use support::FileDatabase;
use support::TestResult;
use turso_db::Value;
use turso_db::params;
use turso_db::transaction::TransactionBehavior;

const CORE_MIGRATION_SQL: &str = include_str!("../migrations/0002_attention_core.sql");
const DELIVERY_MIGRATION_SQL: &str = include_str!("../migrations/0003_durable_delivery.sql");

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
async fn durable_delivery_schema_enforces_shape_width_bounds_and_foreign_keys() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let connection = fixture.database.connect()?;
    connection.execute("PRAGMA foreign_keys = ON", ()).await?;
    connection.execute_batch(CORE_MIGRATION_SQL).await?;
    connection.execute_batch(DELIVERY_MIGRATION_SQL).await?;
    connection
        .execute(
            "INSERT INTO change_events
             (cursor, event_id, occurred_at, kind, payload_version, payload_bytes)
             VALUES (?1, 'event-1', '2026-08-04T00:00:00Z', 0, 1, x'01')",
            params![2_u64.to_be_bytes().to_vec()],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO outbox_intents
             (id, deduplication_key, subject_kind, subject_id, originating_event_id, created_at, purpose)
             VALUES ('intent-1', 'dedupe-1', 0, 'signal-1', 'event-1', '2026-08-04T00:00:00Z', 0)",
            (),
        )
        .await?;
    connection
        .execute(
            "INSERT INTO delivery_states (intent_id, status) VALUES ('intent-1', 0)",
            (),
        )
        .await?;

    assert!(
        connection
            .execute(
                "UPDATE delivery_states SET status = 1, lease_token = ?1 WHERE intent_id = 'intent-1'",
                params![vec![7_u8; 32]],
            )
            .await
            .is_err()
    );
    connection
        .execute(
            "UPDATE delivery_states
             SET status = 1, lease_token = ?1, lease_expires_at = '2026-08-04T00:01:00Z'
             WHERE intent_id = 'intent-1'",
            params![vec![7_u8; 32]],
        )
        .await?;
    assert!(
        connection
            .execute(
                "UPDATE delivery_states SET lease_token = ?1 WHERE intent_id = 'intent-1'",
                params![vec![7_u8; 31]],
            )
            .await
            .is_err()
    );
    assert!(
        connection
            .execute(
                "UPDATE delivery_states
                 SET status = 2, lease_token = NULL, lease_expires_at = NULL,
                     attempt = ?1, error = 'retry', next_retry_at = '2026-08-04T00:02:00Z'
                 WHERE intent_id = 'intent-1'",
                params![4_294_967_296_i64],
            )
            .await
            .is_err()
    );
    assert!(
        connection
            .execute(
                "UPDATE delivery_states
                 SET status = 2, lease_token = NULL, lease_expires_at = NULL,
                     attempt = 1, error = ?1, next_retry_at = '2026-08-04T00:02:00Z'
                 WHERE intent_id = 'intent-1'",
                params!["x".repeat(65_537)],
            )
            .await
            .is_err()
    );
    assert!(
        connection
            .execute(
                "INSERT INTO delivery_states (intent_id, status) VALUES ('missing-intent', 0)",
                (),
            )
            .await
            .is_err()
    );
    assert!(
        connection
            .execute(
                "INSERT INTO delivery_checkpoints (worker, cursor) VALUES ('worker', ?1)",
                params![vec![0_u8; 7]],
            )
            .await
            .is_err()
    );
    assert!(
        connection
            .execute(
                "INSERT INTO delivery_checkpoints (worker, cursor) VALUES (?1, ?2)",
                params!["w".repeat(65_537), vec![0_u8; 8]],
            )
            .await
            .is_err()
    );
    connection
        .execute(
            "INSERT INTO delivery_checkpoints (worker, cursor) VALUES ('worker', ?1)",
            params![vec![0_u8; 8]],
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn current_fire_partial_index_allows_history_and_arbitrates_connections() -> TestResult {
    let fixture = FileDatabase::open().await?;
    let first = fixture.database.connect()?;
    first.execute_batch(CORE_MIGRATION_SQL).await?;
    first.execute_batch(DELIVERY_MIGRATION_SQL).await?;
    first
        .execute(
            "INSERT INTO reminders
             (id, revision, target_kind, target_id, trigger_at, current_fire_id)
             VALUES ('reminder-1', ?1, 0, 'target-1', '2026-08-04T00:00:00Z', NULL)",
            params![1_u64.to_be_bytes().to_vec()],
        )
        .await?;
    for (id, ordinal, state) in [
        ("history-ack", 0_i64, 2_i64),
        ("history-snooze", 1, 3),
        ("current", 2, 0),
    ] {
        first
            .execute(
                "INSERT INTO reminder_fires
                 (id, reminder_id, ordinal, trigger_at, state)
                 VALUES (?1, 'reminder-1', ?2, '2026-08-04T00:00:00Z', ?3)",
                params![id, ordinal, state],
            )
            .await?;
    }

    let second = fixture.database.connect()?;
    assert!(
        second
            .execute(
                "INSERT INTO reminder_fires
                 (id, reminder_id, ordinal, trigger_at, state)
                 VALUES ('second-current', 'reminder-1', 3, '2026-08-04T00:00:00Z', 1)",
                (),
            )
            .await
            .is_err()
    );
    first
        .execute(
            "UPDATE reminder_fires SET state = 2 WHERE id = 'current'",
            (),
        )
        .await?;
    second
        .execute(
            "INSERT INTO reminder_fires
             (id, reminder_id, ordinal, trigger_at, state)
             VALUES ('replacement', 'reminder-1', 3, '2026-08-04T00:00:00Z', 1)",
            (),
        )
        .await?;
    assert!(
        first
            .execute(
                "UPDATE reminder_fires SET state = 0 WHERE id = 'history-ack'",
                (),
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
