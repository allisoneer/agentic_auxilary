use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use chrono::Utc;
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

#[tokio::test]
async fn roots_outcomes_events_and_original_views_survive_restart() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config).await?;
    database.run_startup_migrations().await?;
    let command = CreateWorkItem::new(
        WorkItemId::new(),
        None,
        None,
        None,
        None,
        MutationIdempotencyKey::new(),
    );
    let bundle = evaluate_create_work_item(
        &command,
        EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
    );
    let original_root = bundle.root().clone();
    let applied = database.commit_create_work_item(bundle.clone()).await?;
    let current = database.work_item(command.id()).await?.expect("work item");
    database
        .commit_complete_work_item(evaluate_complete_work_item(
            &CompleteWorkItem::new(
                current.id(),
                current.revision(),
                MutationIdempotencyKey::new(),
            ),
            &current,
            EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
        )?)
        .await?;
    database.close().await?;
    database.reopen().await?;
    assert_eq!(
        database
            .work_item(command.id())
            .await?
            .expect("completed work item")
            .lifecycle(),
        WorkItemLifecycle::Completed
    );
    assert_eq!(
        database.commit_create_work_item(bundle).await?,
        applied.replayed()
    );
    let changes = database
        .changes_after(ChangesAfterQuery::new(
            CommitCursor::try_from(1)?,
            QueryLimit::try_from(8)?,
        ))
        .await?;
    let ChangesResult::Page(page) = changes else {
        return Err("expected page".into());
    };
    assert_eq!(page.events().len(), 2);
    assert_eq!(
        page.events()[0].draft().affected_views(),
        &[AffectedView::WorkItem {
            work_item: original_root,
        }]
    );
    database.close().await?;
    Ok(())
}

#[tokio::test]
async fn source_authority_outbox_identity_and_complete_fire_history_survive_restart() -> TestResult
{
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config).await?;
    database.run_startup_migrations().await?;
    let time = chrono::DateTime::parse_from_rfc3339("2026-08-03T12:00:00Z")?.with_timezone(&Utc);
    let kind = SourceKind::new("linear")?;
    let instance = SourceInstance::new("workspace")?;
    let entity_key = SourceEntityKey::new(
        kind.clone(),
        instance.clone(),
        ExternalEntityId::new("ENG-1120")?,
    );
    let mutation = MutationIdempotencyKey::new();
    let receipt_id = SourceReceiptId::new();
    let signal_id = AttentionSignalId::new();
    let outbox_id = OutboxIntentId::new();
    let source = IngestSourceOccurrence::new(
        receipt_id,
        Some(SourceEntityIdentity::new(
            SourceEntityId::new(),
            entity_key.clone(),
        )),
        signal_id,
        OccurrenceKey::new(kind, instance, OccurrenceId::new("restart-1")?),
        time,
        time,
        SourceOrderMode::Ordered {
            domain: SourceComparatorDomain::new("sequence")?,
            value: Some(NormalizedSourceOrder::new([1])?),
        },
        SignalSourceLifecycle::Active,
        true,
        mutation,
    );
    let source_outcome = database
        .commit_ingest_source_occurrence(evaluate_ingest_source_occurrence(
            &source,
            None,
            None,
            EvaluationContext::new(ChangeEventId::new(), Some(outbox_id), time),
        )?)
        .await?;
    assert_eq!(source_outcome.outbox_intent_id(), Some(outbox_id));

    let reminder_id = ReminderId::new();
    let fire_id = ReminderFireId::new();
    database
        .commit_create_reminder(evaluate_create_reminder(
            &CreateReminder::new(
                reminder_id,
                fire_id,
                ReminderTarget::AttentionSignal(signal_id),
                time,
                MutationIdempotencyKey::new(),
            ),
            EvaluationContext::new(ChangeEventId::new(), None, time),
        ))
        .await?;
    let reminder = database.reminder(reminder_id).await?.expect("reminder");
    database
        .commit_fire_reminder(evaluate_fire_reminder(
            &FireReminder::new(reminder_id, fire_id, MutationIdempotencyKey::new()),
            &reminder,
            EvaluationContext::new(ChangeEventId::new(), None, time),
        )?)
        .await?;
    let reminder = database
        .reminder(reminder_id)
        .await?
        .expect("fired reminder");
    let replacement = ReminderFireId::new();
    database
        .commit_snooze_reminder_fire(evaluate_snooze_reminder_fire(
            &SnoozeReminderFire::new(
                reminder_id,
                fire_id,
                replacement,
                time + chrono::Duration::hours(1),
                reminder.revision(),
                MutationIdempotencyKey::new(),
            ),
            &reminder,
            EvaluationContext::new(ChangeEventId::new(), None, time),
        )?)
        .await?;
    let before_restart = database.snapshot().await?.cursor();
    database.close().await?;
    database.reopen().await?;

    assert!(database.source_receipt(receipt_id).await?.is_some());
    assert!(
        database
            .source_entity(SourceAuthorityQuery::new(entity_key))
            .await?
            .is_some()
    );
    let stored = database
        .prior_outcome(PriorOutcomeQuery::new(mutation))
        .await?
        .expect("source outcome");
    let PriorMutationOutcome::IngestSourceOccurrence(stored) = stored else {
        return Err("wrong stored source outcome".into());
    };
    assert_eq!(stored.outbox_intent_id(), Some(outbox_id));
    let reminder = database.reminder(reminder_id).await?.expect("reminder");
    assert_eq!(reminder.fires().len(), 2);
    assert_eq!(reminder.fires()[0].id(), fire_id);
    assert_eq!(reminder.fires()[1].id(), replacement);
    assert_eq!(database.snapshot().await?.cursor(), before_restart);
    database.close().await?;
    Ok(())
}
