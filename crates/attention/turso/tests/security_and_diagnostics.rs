use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use chrono::DateTime;
use chrono::Utc;
use std::error::Error;
use turso_db::Builder;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

async fn pending_delivery(
    database: &AttentionDatabase,
    sequence: usize,
    time: DateTime<Utc>,
) -> TestResult<OutboxIntentId> {
    let intent_id = OutboxIntentId::new();
    let command = IngestSourceOccurrence::new(
        SourceReceiptId::new(),
        None,
        AttentionSignalId::new(),
        OccurrenceKey::new(
            SourceKind::new("security-delivery")?,
            SourceInstance::new(format!("instance-{sequence}"))?,
            OccurrenceId::new(format!("occurrence-{sequence}"))?,
        ),
        time,
        time,
        SourceOrderMode::Unordered,
        SignalSourceLifecycle::Active,
        true,
        MutationIdempotencyKey::new(),
    );
    database
        .commit_ingest_source_occurrence(evaluate_ingest_source_occurrence(
            &command,
            None,
            None,
            EvaluationContext::new(ChangeEventId::new(), Some(intent_id), time),
        )?)
        .await?;
    Ok(intent_id)
}

#[tokio::test]
async fn sql_shaped_source_values_remain_data_and_optional_outbox_is_atomic() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config.clone()).await?;
    database.run_startup_migrations().await?;
    let time = DateTime::parse_from_rfc3339("2026-08-03T12:00:00Z")
        .expect("time")
        .with_timezone(&Utc);
    let source_kind = SourceKind::new("linear'; DROP TABLE work_items;--").expect("kind");
    let instance = SourceInstance::new("workspace\" OR 1=1 --").expect("instance");
    let receipt_id = SourceReceiptId::new();
    let signal_id = AttentionSignalId::new();
    let event_id = ChangeEventId::new();
    let outbox_id = OutboxIntentId::new();
    let command = IngestSourceOccurrence::new(
        receipt_id,
        None,
        signal_id,
        OccurrenceKey::new(
            source_kind,
            instance,
            OccurrenceId::new("occurrence'); DELETE FROM change_events;--").expect("occurrence"),
        ),
        time,
        time,
        SourceOrderMode::Unordered,
        SignalSourceLifecycle::Active,
        true,
        MutationIdempotencyKey::new(),
    );
    let bundle = evaluate_ingest_source_occurrence(
        &command,
        None,
        None,
        EvaluationContext::new(event_id, Some(outbox_id), time),
    )?;
    let result = database.commit_ingest_source_occurrence(bundle).await?;
    assert_eq!(result.outbox_intent_id(), Some(outbox_id));
    assert_eq!(
        database
            .source_receipt(receipt_id)
            .await?
            .expect("receipt")
            .occurrence_key()
            .occurrence_id()
            .as_str(),
        "occurrence'); DELETE FROM change_events;--"
    );
    let changes = database
        .changes_after(ChangesAfterQuery::new(
            CommitCursor::try_from(1)?,
            QueryLimit::try_from(8)?,
        ))
        .await?;
    assert!(matches!(changes, ChangesResult::Page(ref page) if page.events().len() == 1));
    database.close().await?;

    let raw = Builder::new_local(
        config
            .database_directory()
            .database_file()
            .to_str()
            .ok_or("database path is not UTF-8")?,
    )
    .build()
    .await?;
    let connection = raw.connect()?;
    let mut rows = connection
        .query(
            "SELECT d.intent_id, d.status FROM outbox_intents AS o
             JOIN delivery_states AS d ON d.intent_id = o.id WHERE o.id = ?1",
            turso_db::params![outbox_id.to_string()],
        )
        .await?;
    let row = rows.next().await?.ok_or("delivery authority missing")?;
    assert_eq!(row.get::<String>(0)?, outbox_id.to_string());
    assert_eq!(row.get::<i64>(1)?, 0);
    assert!(rows.next().await?.is_none());
    Ok(())
}

#[tokio::test]
async fn delivery_values_are_bound_bounded_raw_and_malformed_rows_fail_closed() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config.clone()).await?;
    database.run_startup_migrations().await?;
    let time = DateTime::parse_from_rfc3339("2026-08-03T12:00:00Z")?.with_timezone(&Utc);
    let succeeded = pending_delivery(&database, 10, time).await?;
    let failed = pending_delivery(&database, 11, time).await?;
    let skipped = pending_delivery(&database, 12, time).await?;
    let oversized = pending_delivery(&database, 13, time).await?;
    let claims = database
        .claim(DeliveryClaimQuery::new(
            time,
            time + chrono::Duration::minutes(1),
            ClaimLimit::try_from(8)?,
        ))
        .await?;
    let token = |id| {
        claims
            .iter()
            .copied()
            .find(|claim| claim.intent_id() == id)
            .map(DeliveryClaim::token)
            .ok_or("claim missing")
    };
    let provider_value = "provider'); DROP TABLE delivery_states;--";
    let error_value = "failure'); DELETE FROM outbox_intents;--";
    let reason_value = "reason\" OR 1=1 --";
    database
        .succeed(
            succeeded,
            token(succeeded)?,
            ProviderMessageId::new(provider_value, 256)?,
            time,
        )
        .await?;
    database
        .fail_terminal(
            failed,
            token(failed)?,
            u32::MAX,
            BoundedDeliveryText::new(error_value, 256)?,
            time,
        )
        .await?;
    database
        .skip(
            skipped,
            token(skipped)?,
            BoundedDeliveryText::new(reason_value, 256)?,
            time,
        )
        .await?;
    let oversized_value = "secret".repeat(attention_turso::DELIVERY_TEXT_LIMIT_BYTES / 6 + 1);
    let oversized_text = BoundedDeliveryText::new(
        oversized_value.clone(),
        attention_turso::DELIVERY_TEXT_LIMIT_BYTES * 2,
    )?;
    let error = database
        .fail_retryable(oversized, token(oversized)?, 1, oversized_text, time)
        .await
        .expect_err("adapter ceiling must reject caller value");
    assert!(!error.to_string().contains("secret"));
    database.close().await?;

    let raw = Builder::new_local(
        config
            .database_directory()
            .database_file()
            .to_str()
            .ok_or("database path is not UTF-8")?,
    )
    .build()
    .await?;
    let connection = raw.connect()?;
    let mut rows = connection
        .query(
            "SELECT length(lease_token) FROM delivery_states WHERE intent_id = ?1",
            turso_db::params![oversized.to_string()],
        )
        .await?;
    assert_eq!(
        rows.next()
            .await?
            .ok_or("raw token row missing")?
            .get::<i64>(0)?,
        32
    );
    drop(rows);
    let mut rows = connection
        .query(
            "SELECT
                 (SELECT provider_message_id FROM delivery_states WHERE intent_id = ?1),
                 (SELECT error FROM delivery_states WHERE intent_id = ?2),
                 (SELECT reason FROM delivery_states WHERE intent_id = ?3)",
            turso_db::params![
                succeeded.to_string(),
                failed.to_string(),
                skipped.to_string()
            ],
        )
        .await?;
    let row = rows.next().await?.ok_or("bound values missing")?;
    assert_eq!(row.get::<String>(0)?, provider_value);
    assert_eq!(row.get::<String>(1)?, error_value);
    assert_eq!(row.get::<String>(2)?, reason_value);
    drop(rows);
    connection
        .execute("PRAGMA ignore_check_constraints = ON", ())
        .await?;
    connection
        .execute(
            "UPDATE delivery_states SET succeeded_at = ?2 WHERE intent_id = ?1",
            turso_db::params![succeeded.to_string(), "stored-secret-timestamp"],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO delivery_checkpoints (worker, cursor) VALUES (?1, ?2)",
            turso_db::params!["malformed-worker", vec![0_u8; 7]],
        )
        .await?;
    drop(connection);
    drop(raw);

    database.reopen().await?;
    let error = database
        .inspect(succeeded)
        .await
        .expect_err("malformed timestamp must fail closed");
    assert!(!error.to_string().contains("stored-secret-timestamp"));
    let worker = BoundedDeliveryText::new("malformed-worker", 128)?;
    let error = database
        .checkpoint(CheckpointQuery::new(worker))
        .await
        .expect_err("malformed cursor must fail closed");
    assert!(!error.to_string().contains("malformed-worker"));
    database.close().await?;
    Ok(())
}
