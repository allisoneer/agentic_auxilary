#![expect(clippy::expect_used, reason = "fixed validated conformance fixtures")]

use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use chrono::DateTime;
use chrono::Utc;
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn at(hour: u32) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&format!("2026-08-03T{hour:02}:00:00Z"))
        .expect("fixed time")
        .with_timezone(&Utc)
}

fn context() -> EvaluationContext {
    EvaluationContext::new(ChangeEventId::new(), None, at(12))
}

async fn database() -> TestResult<(tempfile::TempDir, AttentionDatabase)> {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config).await?;
    database.run_startup_migrations().await?;
    Ok((root, database))
}

fn source_command() -> IngestSourceOccurrence {
    let kind = SourceKind::new("linear").expect("kind");
    let instance = SourceInstance::new("workspace").expect("instance");
    IngestSourceOccurrence::new(
        SourceReceiptId::new(),
        Some(SourceEntityIdentity::new(
            SourceEntityId::new(),
            SourceEntityKey::new(
                kind.clone(),
                instance.clone(),
                ExternalEntityId::new("ENG-1120").expect("external"),
            ),
        )),
        AttentionSignalId::new(),
        OccurrenceKey::new(
            kind,
            instance,
            OccurrenceId::new("occurrence-1").expect("occurrence"),
        ),
        at(11),
        at(12),
        SourceOrderMode::Ordered {
            domain: SourceComparatorDomain::new("sequence").expect("domain"),
            value: Some(NormalizedSourceOrder::new([1]).expect("order")),
        },
        SignalSourceLifecycle::Active,
        false,
        MutationIdempotencyKey::new(),
    )
}

fn retry_source(command: &IngestSourceOccurrence, order: u8) -> IngestSourceOccurrence {
    IngestSourceOccurrence::new(
        command.receipt_id(),
        command.entity().cloned(),
        command.signal_id(),
        command.occurrence_key().clone(),
        *command.occurred_at(),
        *command.ingested_at(),
        SourceOrderMode::Ordered {
            domain: SourceComparatorDomain::new("sequence").expect("domain"),
            value: Some(NormalizedSourceOrder::new([order]).expect("order")),
        },
        command.source_lifecycle(),
        command.fresh_attention(),
        MutationIdempotencyKey::new(),
    )
}

#[tokio::test]
async fn all_nine_commits_apply_replay_and_preserve_history() -> TestResult {
    let (_root, database) = database().await?;

    let create = CreateWorkItem::new(
        WorkItemId::new(),
        Some(at(13)),
        None,
        None,
        None,
        MutationIdempotencyKey::new(),
    );
    let create_bundle = evaluate_create_work_item(&create, context());
    let applied = database
        .commit_create_work_item(create_bundle.clone())
        .await?;
    assert_eq!(applied.disposition(), CommandDisposition::Applied);
    let replay = database.commit_create_work_item(create_bundle).await?;
    assert_eq!(replay, applied.replayed());
    let current = database.work_item(create.id()).await?.expect("work item");
    let complete = evaluate_complete_work_item(
        &CompleteWorkItem::new(
            current.id(),
            current.revision(),
            MutationIdempotencyKey::new(),
        ),
        &current,
        context(),
    )?;
    let completed = database.commit_complete_work_item(complete.clone()).await?;
    assert_eq!(
        database.commit_complete_work_item(complete).await?,
        completed.replayed()
    );

    let cancel_create = CreateWorkItem::new(
        WorkItemId::new(),
        None,
        None,
        None,
        None,
        MutationIdempotencyKey::new(),
    );
    database
        .commit_create_work_item(evaluate_create_work_item(&cancel_create, context()))
        .await?;
    let current = database
        .work_item(cancel_create.id())
        .await?
        .expect("cancel work item");
    let cancel = evaluate_cancel_work_item(
        &CancelWorkItem::new(
            current.id(),
            current.revision(),
            MutationIdempotencyKey::new(),
        ),
        &current,
        context(),
    )?;
    let cancelled = database.commit_cancel_work_item(cancel.clone()).await?;
    assert_eq!(
        database.commit_cancel_work_item(cancel).await?,
        cancelled.replayed()
    );

    let source = source_command();
    let source_bundle = evaluate_ingest_source_occurrence(&source, None, None, context())?;
    let ingested = database
        .commit_ingest_source_occurrence(source_bundle.clone())
        .await?;
    assert_eq!(
        database
            .commit_ingest_source_occurrence(source_bundle)
            .await?,
        ingested.replayed()
    );
    let signal = database
        .attention_signal(source.signal_id())
        .await?
        .expect("signal");
    let acknowledge = evaluate_acknowledge_attention_signal(
        &AcknowledgeAttentionSignal::new(
            signal.id(),
            signal.revision(),
            MutationIdempotencyKey::new(),
        ),
        &signal,
        context(),
    )?;
    let acknowledged = database
        .commit_acknowledge_attention_signal(acknowledge.clone())
        .await?;
    assert_eq!(
        database
            .commit_acknowledge_attention_signal(acknowledge)
            .await?,
        acknowledged.replayed()
    );

    let reminder_id = ReminderId::new();
    let fire_id = ReminderFireId::new();
    let create_reminder = evaluate_create_reminder(
        &CreateReminder::new(
            reminder_id,
            fire_id,
            ReminderTarget::WorkItem(create.id()),
            at(14),
            MutationIdempotencyKey::new(),
        ),
        context(),
    );
    let reminder_created = database
        .commit_create_reminder(create_reminder.clone())
        .await?;
    assert_eq!(
        database.commit_create_reminder(create_reminder).await?,
        reminder_created.replayed()
    );
    let reminder = database.reminder(reminder_id).await?.expect("reminder");
    let fire = evaluate_fire_reminder(
        &FireReminder::new(reminder_id, fire_id, MutationIdempotencyKey::new()),
        &reminder,
        context(),
    )?;
    let fired = database.commit_fire_reminder(fire.clone()).await?;
    assert_eq!(database.commit_fire_reminder(fire).await?, fired.replayed());
    let reminder = database
        .reminder(reminder_id)
        .await?
        .expect("fired reminder");
    let snooze = evaluate_snooze_reminder_fire(
        &SnoozeReminderFire::new(
            reminder_id,
            fire_id,
            ReminderFireId::new(),
            at(15),
            reminder.revision(),
            MutationIdempotencyKey::new(),
        ),
        &reminder,
        context(),
    )?;
    let snoozed = database.commit_snooze_reminder_fire(snooze.clone()).await?;
    assert_eq!(
        database.commit_snooze_reminder_fire(snooze).await?,
        snoozed.replayed()
    );

    let reminder_id = ReminderId::new();
    let fire_id = ReminderFireId::new();
    database
        .commit_create_reminder(evaluate_create_reminder(
            &CreateReminder::new(
                reminder_id,
                fire_id,
                ReminderTarget::WorkItem(cancel_create.id()),
                at(16),
                MutationIdempotencyKey::new(),
            ),
            context(),
        ))
        .await?;
    let reminder = database.reminder(reminder_id).await?.expect("reminder");
    database
        .commit_fire_reminder(evaluate_fire_reminder(
            &FireReminder::new(reminder_id, fire_id, MutationIdempotencyKey::new()),
            &reminder,
            context(),
        )?)
        .await?;
    let reminder = database
        .reminder(reminder_id)
        .await?
        .expect("fired reminder");
    let acknowledge = evaluate_acknowledge_reminder_fire(
        &AcknowledgeReminderFire::new(
            reminder_id,
            fire_id,
            reminder.revision(),
            MutationIdempotencyKey::new(),
        ),
        &reminder,
        context(),
    )?;
    let acknowledged = database
        .commit_acknowledge_reminder_fire(acknowledge.clone())
        .await?;
    assert_eq!(
        database
            .commit_acknowledge_reminder_fire(acknowledge)
            .await?,
        acknowledged.replayed()
    );

    let snapshot = database.snapshot().await?;
    let changes = database
        .changes_after(ChangesAfterQuery::new(
            CommitCursor::try_from(1)?,
            QueryLimit::try_from(64)?,
        ))
        .await?;
    let ChangesResult::Page(page) = changes else {
        return Err("expected change page".into());
    };
    assert_eq!(page.resume_after(), snapshot.cursor());
    assert_eq!(page.events().len(), 12);
    assert_eq!(snapshot.reminders().len(), 2);
    database.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_mutation_key_has_one_applied_result_and_one_replay() -> TestResult {
    let (_root, database) = database().await?;
    let command = CreateWorkItem::new(
        WorkItemId::new(),
        None,
        None,
        None,
        None,
        MutationIdempotencyKey::new(),
    );
    let bundle = evaluate_create_work_item(&command, context());
    let first_database = database.clone();
    let first_bundle = bundle.clone();
    let first =
        tokio::spawn(async move { first_database.commit_create_work_item(first_bundle).await });
    let second_database = database.clone();
    let second = tokio::spawn(async move { second_database.commit_create_work_item(bundle).await });
    let first = first.await??;
    let second = second.await??;
    assert_ne!(first.disposition(), second.disposition());
    assert_eq!(first.cursor(), second.cursor());
    database.close().await?;
    Ok(())
}

#[tokio::test]
async fn mismatches_and_failed_guards_write_nothing() -> TestResult {
    let (_root, database) = database().await?;
    let mutation = MutationIdempotencyKey::new();
    let first = CreateWorkItem::new(WorkItemId::new(), None, None, None, None, mutation);
    database
        .commit_create_work_item(evaluate_create_work_item(&first, context()))
        .await?;
    let changed = CreateWorkItem::new(WorkItemId::new(), None, None, None, None, mutation);
    assert!(matches!(
        database
            .commit_create_work_item(evaluate_create_work_item(&changed, context()))
            .await,
        Err(PortError::Semantic(SemanticError::IdempotencyMismatch(key))) if key == mutation
    ));

    let current = database.work_item(first.id()).await?.expect("work item");
    let first_completion = evaluate_complete_work_item(
        &CompleteWorkItem::new(
            current.id(),
            current.revision(),
            MutationIdempotencyKey::new(),
        ),
        &current,
        context(),
    )?;
    let stale_key = MutationIdempotencyKey::new();
    let stale = evaluate_complete_work_item(
        &CompleteWorkItem::new(current.id(), current.revision(), stale_key),
        &current,
        context(),
    )?;
    database.commit_complete_work_item(first_completion).await?;
    let before = database.snapshot().await?.cursor();
    assert!(matches!(
        database.commit_complete_work_item(stale).await,
        Err(PortError::Semantic(
            SemanticError::ExpectedRevisionConflict { .. }
        ))
    ));
    assert_eq!(database.snapshot().await?.cursor(), before);
    assert!(
        database
            .prior_outcome(PriorOutcomeQuery::new(stale_key))
            .await?
            .is_none()
    );

    let target = ReminderTarget::WorkItem(first.id());
    database
        .commit_create_reminder(evaluate_create_reminder(
            &CreateReminder::new(
                ReminderId::new(),
                ReminderFireId::new(),
                target,
                at(14),
                MutationIdempotencyKey::new(),
            ),
            context(),
        ))
        .await?;
    assert!(matches!(
        database
            .commit_create_reminder(evaluate_create_reminder(
                &CreateReminder::new(
                    ReminderId::new(),
                    ReminderFireId::new(),
                    target,
                    at(15),
                    MutationIdempotencyKey::new(),
                ),
                context(),
            ))
            .await,
        Err(PortError::Semantic(SemanticError::CreateConflict(
            ResourceRef::Reminder(_)
        )))
    ));
    database.close().await?;
    Ok(())
}

#[tokio::test]
async fn duplicate_occurrence_uses_original_outcome_without_storing_new_key() -> TestResult {
    let (_root, database) = database().await?;
    let command = source_command();
    let bundle = evaluate_ingest_source_occurrence(&command, None, None, context())?;
    let applied = database.commit_ingest_source_occurrence(bundle).await?;
    let entity = database
        .source_entity(SourceAuthorityQuery::new(
            command.entity().expect("entity").key().clone(),
        ))
        .await?
        .expect("entity");
    let signal = database
        .attention_signal(command.signal_id())
        .await?
        .expect("signal");

    let duplicate = retry_source(&command, 1);
    let duplicate_key = duplicate.idempotency_key();
    let replay = database
        .commit_ingest_source_occurrence(evaluate_ingest_source_occurrence(
            &duplicate,
            Some(&entity),
            Some(&signal),
            context(),
        )?)
        .await?;
    assert_eq!(replay, applied.replayed());
    assert!(
        database
            .prior_outcome(PriorOutcomeQuery::new(duplicate_key))
            .await?
            .is_none()
    );

    let changed = retry_source(&command, 2);
    assert!(matches!(
        database
            .commit_ingest_source_occurrence(evaluate_ingest_source_occurrence(
                &changed,
                Some(&entity),
                Some(&signal),
                context(),
            )?)
            .await,
        Err(PortError::Semantic(
            SemanticError::OccurrenceContentMismatch(_)
        ))
    ));
    database.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_occurrence_revision_target_and_current_fire_have_one_winner() -> TestResult {
    let (_root, database) = database().await?;

    let source = source_command();
    let first_source = evaluate_ingest_source_occurrence(&source, None, None, context())?;
    let duplicate_source =
        evaluate_ingest_source_occurrence(&retry_source(&source, 1), None, None, context())?;
    let first_database = database.clone();
    let first = tokio::spawn(async move {
        first_database
            .commit_ingest_source_occurrence(first_source)
            .await
    });
    let second_database = database.clone();
    let second = tokio::spawn(async move {
        second_database
            .commit_ingest_source_occurrence(duplicate_source)
            .await
    });
    let first = first.await??;
    let second = second.await??;
    assert_ne!(first.disposition(), second.disposition());
    assert_eq!(first.cursor(), second.cursor());

    let create = CreateWorkItem::new(
        WorkItemId::new(),
        None,
        None,
        None,
        None,
        MutationIdempotencyKey::new(),
    );
    database
        .commit_create_work_item(evaluate_create_work_item(&create, context()))
        .await?;
    let current = database.work_item(create.id()).await?.expect("work item");
    let complete = evaluate_complete_work_item(
        &CompleteWorkItem::new(
            current.id(),
            current.revision(),
            MutationIdempotencyKey::new(),
        ),
        &current,
        context(),
    )?;
    let cancel = evaluate_cancel_work_item(
        &CancelWorkItem::new(
            current.id(),
            current.revision(),
            MutationIdempotencyKey::new(),
        ),
        &current,
        context(),
    )?;
    let first_database = database.clone();
    let first =
        tokio::spawn(async move { first_database.commit_complete_work_item(complete).await });
    let second_database = database.clone();
    let second = tokio::spawn(async move { second_database.commit_cancel_work_item(cancel).await });
    let revision_results = [first.await?.map(|_| ()), second.await?.map(|_| ())];
    assert_eq!(
        revision_results
            .iter()
            .filter(|result| result.is_ok())
            .count(),
        1
    );
    assert!(revision_results.iter().any(|result| matches!(
        result,
        Err(PortError::Semantic(
            SemanticError::ExpectedRevisionConflict { .. }
        ))
    )));

    let target = ReminderTarget::WorkItem(WorkItemId::new());
    let first_reminder = evaluate_create_reminder(
        &CreateReminder::new(
            ReminderId::new(),
            ReminderFireId::new(),
            target,
            at(14),
            MutationIdempotencyKey::new(),
        ),
        context(),
    );
    let second_reminder = evaluate_create_reminder(
        &CreateReminder::new(
            ReminderId::new(),
            ReminderFireId::new(),
            target,
            at(15),
            MutationIdempotencyKey::new(),
        ),
        context(),
    );
    let first_database = database.clone();
    let first =
        tokio::spawn(async move { first_database.commit_create_reminder(first_reminder).await });
    let second_database = database.clone();
    let second = tokio::spawn(async move {
        second_database
            .commit_create_reminder(second_reminder)
            .await
    });
    let target_results = [first.await?.map(|_| ()), second.await?.map(|_| ())];
    assert_eq!(
        target_results
            .iter()
            .filter(|result| result.is_ok())
            .count(),
        1
    );
    assert!(target_results.iter().any(|result| matches!(
        result,
        Err(PortError::Semantic(SemanticError::CreateConflict(
            ResourceRef::Reminder(_)
        )))
    )));

    let reminder_id = ReminderId::new();
    let fire_id = ReminderFireId::new();
    database
        .commit_create_reminder(evaluate_create_reminder(
            &CreateReminder::new(
                reminder_id,
                fire_id,
                ReminderTarget::WorkItem(WorkItemId::new()),
                at(16),
                MutationIdempotencyKey::new(),
            ),
            context(),
        ))
        .await?;
    let reminder = database.reminder(reminder_id).await?.expect("reminder");
    let first_fire = evaluate_fire_reminder(
        &FireReminder::new(reminder_id, fire_id, MutationIdempotencyKey::new()),
        &reminder,
        context(),
    )?;
    let second_fire = evaluate_fire_reminder(
        &FireReminder::new(reminder_id, fire_id, MutationIdempotencyKey::new()),
        &reminder,
        context(),
    )?;
    let first_database = database.clone();
    let first = tokio::spawn(async move { first_database.commit_fire_reminder(first_fire).await });
    let second_database = database.clone();
    let second =
        tokio::spawn(async move { second_database.commit_fire_reminder(second_fire).await });
    let fire_results = [first.await?.map(|_| ()), second.await?.map(|_| ())];
    assert_eq!(
        fire_results.iter().filter(|result| result.is_ok()).count(),
        1
    );
    assert!(fire_results.iter().any(|result| matches!(
        result,
        Err(PortError::Semantic(
            SemanticError::ExpectedRevisionConflict { .. }
        ))
    )));
    database.close().await?;
    Ok(())
}
