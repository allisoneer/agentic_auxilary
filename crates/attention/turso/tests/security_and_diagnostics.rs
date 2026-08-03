use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use chrono::DateTime;
use chrono::Utc;
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

#[tokio::test]
async fn sql_shaped_source_values_remain_data_and_optional_outbox_is_atomic() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config).await?;
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
    Ok(())
}
