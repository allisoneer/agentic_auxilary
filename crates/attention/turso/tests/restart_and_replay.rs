use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use chrono::Utc;
use std::error::Error;
use turso_db::Builder;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

async fn pending_delivery(
    database: &AttentionDatabase,
    sequence: usize,
    time: chrono::DateTime<Utc>,
) -> TestResult<OutboxIntentId> {
    let intent_id = OutboxIntentId::new();
    let command = IngestSourceOccurrence::new(
        SourceReceiptId::new(),
        None,
        AttentionSignalId::new(),
        OccurrenceKey::new(
            SourceKind::new("restart-delivery")?,
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
    let database = AttentionDatabase::open(config.clone()).await?;
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
    let reminder_outbox_id = OutboxIntentId::new();
    database
        .commit_fire_reminder(evaluate_fire_reminder(
            &FireReminder::new(reminder_id, fire_id, MutationIdempotencyKey::new()),
            &reminder,
            EvaluationContext::new(ChangeEventId::new(), Some(reminder_outbox_id), time),
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

    let acknowledged_reminder_id = ReminderId::new();
    let acknowledged_fire_id = ReminderFireId::new();
    database
        .commit_create_reminder(evaluate_create_reminder(
            &CreateReminder::new(
                acknowledged_reminder_id,
                acknowledged_fire_id,
                ReminderTarget::WorkItem(WorkItemId::new()),
                time,
                MutationIdempotencyKey::new(),
            ),
            EvaluationContext::new(ChangeEventId::new(), None, time),
        ))
        .await?;
    let acknowledged_reminder = database
        .reminder(acknowledged_reminder_id)
        .await?
        .expect("acknowledgement reminder");
    let acknowledged_outbox_id = OutboxIntentId::new();
    database
        .commit_fire_reminder(evaluate_fire_reminder(
            &FireReminder::new(
                acknowledged_reminder_id,
                acknowledged_fire_id,
                MutationIdempotencyKey::new(),
            ),
            &acknowledged_reminder,
            EvaluationContext::new(ChangeEventId::new(), Some(acknowledged_outbox_id), time),
        )?)
        .await?;
    let acknowledged_reminder = database
        .reminder(acknowledged_reminder_id)
        .await?
        .expect("fired acknowledgement reminder");
    database
        .commit_acknowledge_reminder_fire(evaluate_acknowledge_reminder_fire(
            &AcknowledgeReminderFire::new(
                acknowledged_reminder_id,
                acknowledged_fire_id,
                acknowledged_reminder.revision(),
                MutationIdempotencyKey::new(),
            ),
            &acknowledged_reminder,
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
    for intent_id in [outbox_id, reminder_outbox_id, acknowledged_outbox_id] {
        let mut rows = connection
            .query(
                "SELECT status FROM delivery_states WHERE intent_id = ?1",
                turso_db::params![intent_id.to_string()],
            )
            .await?;
        assert_eq!(
            rows.next()
                .await?
                .ok_or("delivery authority missing after lifecycle transition")?
                .get::<i64>(0)?,
            0
        );
    }
    Ok(())
}

#[tokio::test]
async fn every_delivery_state_and_checkpoint_survives_without_repair_rewrite() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config).await?;
    database.run_startup_migrations().await?;
    let base = chrono::DateTime::parse_from_rfc3339("2026-08-03T12:00:00Z")?.with_timezone(&Utc);
    let active = pending_delivery(&database, 20, base).await?;
    let expired = pending_delivery(&database, 21, base).await?;
    let future_retry = pending_delivery(&database, 22, base).await?;
    let due_retry = pending_delivery(&database, 23, base).await?;
    let succeeded = pending_delivery(&database, 24, base).await?;
    let skipped = pending_delivery(&database, 25, base).await?;
    let terminal = pending_delivery(&database, 26, base).await?;
    let claims = database
        .claim(DeliveryClaimQuery::new(
            base,
            base + chrono::Duration::minutes(10),
            ClaimLimit::try_from(16)?,
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
    database
        .renew(
            expired,
            token(expired)?,
            base + chrono::Duration::minutes(5),
        )
        .await?;
    database
        .fail_retryable(
            future_retry,
            token(future_retry)?,
            1,
            BoundedDeliveryText::new("future", 128)?,
            base + chrono::Duration::minutes(6),
        )
        .await?;
    database
        .fail_retryable(
            due_retry,
            token(due_retry)?,
            1,
            BoundedDeliveryText::new("due", 128)?,
            base + chrono::Duration::minutes(5),
        )
        .await?;
    database
        .succeed(
            succeeded,
            token(succeeded)?,
            ProviderMessageId::new("restart-provider", 128)?,
            base,
        )
        .await?;
    database
        .skip(
            skipped,
            token(skipped)?,
            BoundedDeliveryText::new("restart-skip", 128)?,
            base,
        )
        .await?;
    database
        .fail_terminal(
            terminal,
            token(terminal)?,
            3,
            BoundedDeliveryText::new("restart-terminal", 128)?,
            base,
        )
        .await?;
    let pending = pending_delivery(&database, 27, base).await?;
    let worker = BoundedDeliveryText::new("restart-worker", 128)?;
    database
        .advance_checkpoint(CheckpointAdvance::new(
            worker.clone(),
            None,
            CommitCursor::try_from(42)?,
            succeeded,
        ))
        .await?;
    let before_restart = database.snapshot().await?.cursor();
    let active_token = token(active)?;
    let expired_token = token(expired)?;
    database.close().await?;
    database.reopen().await?;

    assert_eq!(database.snapshot().await?.cursor(), before_restart);
    assert!(matches!(
        database.inspect(active).await?.expect("active").state().status(),
        DeliveryStatus::Leased { token, .. } if *token == active_token
    ));
    assert!(matches!(
        database.inspect(expired).await?.expect("expired").state().status(),
        DeliveryStatus::Leased { token, .. } if *token == expired_token
    ));
    assert!(matches!(
        database
            .inspect(future_retry)
            .await?
            .expect("future")
            .state()
            .status(),
        DeliveryStatus::Retryable { .. }
    ));
    assert!(matches!(
        database
            .inspect(due_retry)
            .await?
            .expect("due")
            .state()
            .status(),
        DeliveryStatus::Retryable { .. }
    ));
    assert!(matches!(
        database
            .inspect(succeeded)
            .await?
            .expect("success")
            .state()
            .status(),
        DeliveryStatus::Succeeded { .. }
    ));
    assert!(matches!(
        database
            .inspect(skipped)
            .await?
            .expect("skip")
            .state()
            .status(),
        DeliveryStatus::Skipped { .. }
    ));
    assert!(matches!(
        database
            .inspect(terminal)
            .await?
            .expect("terminal")
            .state()
            .status(),
        DeliveryStatus::TerminalFailure { .. }
    ));
    assert!(matches!(
        database
            .inspect(pending)
            .await?
            .expect("pending")
            .state()
            .status(),
        DeliveryStatus::Pending
    ));
    assert_eq!(
        database
            .checkpoint(CheckpointQuery::new(worker))
            .await?
            .ok_or("checkpoint missing")?
            .cursor(),
        CommitCursor::try_from(42)?
    );

    let reclaimed = database
        .claim(DeliveryClaimQuery::new(
            base + chrono::Duration::minutes(5),
            base + chrono::Duration::minutes(15),
            ClaimLimit::try_from(16)?,
        ))
        .await?;
    let reclaimed_ids = reclaimed
        .iter()
        .map(|claim| claim.intent_id())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(reclaimed_ids.len(), 3);
    assert!(reclaimed_ids.contains(&expired));
    assert!(reclaimed_ids.contains(&due_retry));
    assert!(reclaimed_ids.contains(&pending));
    assert!(!reclaimed_ids.contains(&active));
    assert!(!reclaimed_ids.contains(&future_retry));
    assert_ne!(
        reclaimed
            .iter()
            .find(|claim| claim.intent_id() == expired)
            .ok_or("expired lease not reclaimed")?
            .token(),
        expired_token
    );
    database.close().await?;
    Ok(())
}
