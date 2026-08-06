use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use chrono::DateTime;
use chrono::Utc;
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn at(value: &str) -> TestResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

async fn pending(database: &AttentionDatabase, sequence: usize) -> TestResult<OutboxIntentId> {
    let time = at("2026-08-03T12:00:00Z")?;
    let intent_id = OutboxIntentId::new();
    let command = IngestSourceOccurrence::new(
        SourceReceiptId::new(),
        None,
        AttentionSignalId::new(),
        OccurrenceKey::new(
            SourceKind::new("checkpoint-test")?,
            SourceInstance::new(format!("worker-{sequence}"))?,
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
async fn checkpoint_cas_terminal_gate_precedence_lower_cursor_and_restart() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config).await?;
    database.run_startup_migrations().await?;
    let ids = [
        pending(&database, 1).await?,
        pending(&database, 2).await?,
        pending(&database, 3).await?,
        pending(&database, 4).await?,
    ];
    let before = database.snapshot().await?.cursor();
    let lease_at = at("2026-08-03T12:01:00Z")?;
    let claims = database
        .claim(DeliveryClaimQuery::new(
            lease_at,
            at("2026-08-03T12:02:00Z")?,
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
    database
        .succeed(
            ids[0],
            token(ids[0])?,
            ProviderMessageId::new("provider-before-checkpoint", 128)?,
            lease_at,
        )
        .await?;
    database
        .skip(
            ids[1],
            token(ids[1])?,
            BoundedDeliveryText::new("skip", 128)?,
            lease_at,
        )
        .await?;
    database
        .fail_terminal(
            ids[2],
            token(ids[2])?,
            1,
            BoundedDeliveryText::new("terminal", 128)?,
            lease_at,
        )
        .await?;
    assert!(matches!(
        database.inspect(ids[0]).await?.expect("success").state().status(),
        DeliveryStatus::Succeeded { provider_message_id, .. }
            if provider_message_id.as_str() == "provider-before-checkpoint"
    ));

    let worker = BoundedDeliveryText::new("delivery-worker", 128)?;
    assert!(
        database
            .checkpoint(CheckpointQuery::new(worker.clone()))
            .await?
            .is_none()
    );
    assert_eq!(
        database
            .advance_checkpoint(CheckpointAdvance::new(
                worker.clone(),
                None,
                CommitCursor::try_from(10)?,
                OutboxIntentId::new(),
            ))
            .await?,
        CheckpointAdvanceOutcome::TerminalStateRequired
    );
    assert_eq!(
        database
            .advance_checkpoint(CheckpointAdvance::new(
                worker.clone(),
                None,
                CommitCursor::try_from(10)?,
                ids[3],
            ))
            .await?,
        CheckpointAdvanceOutcome::TerminalStateRequired
    );
    assert_eq!(
        database
            .advance_checkpoint(CheckpointAdvance::new(
                worker.clone(),
                None,
                CommitCursor::try_from(10)?,
                ids[0],
            ))
            .await?,
        CheckpointAdvanceOutcome::Advanced
    );
    assert_eq!(
        database
            .advance_checkpoint(CheckpointAdvance::new(
                worker.clone(),
                Some(CommitCursor::try_from(999)?),
                CommitCursor::try_from(10)?,
                ids[1],
            ))
            .await?,
        CheckpointAdvanceOutcome::Repeated
    );
    assert_eq!(
        database
            .advance_checkpoint(CheckpointAdvance::new(
                worker.clone(),
                Some(CommitCursor::try_from(9)?),
                CommitCursor::try_from(11)?,
                ids[2],
            ))
            .await?,
        CheckpointAdvanceOutcome::Conflict
    );
    assert_eq!(
        database
            .advance_checkpoint(CheckpointAdvance::new(
                worker.clone(),
                Some(CommitCursor::try_from(10)?),
                CommitCursor::try_from(5)?,
                ids[2],
            ))
            .await?,
        CheckpointAdvanceOutcome::Advanced
    );
    assert_eq!(database.snapshot().await?.cursor(), before);

    for (index, terminal_id) in ids[..3].iter().copied().enumerate() {
        let terminal_worker = BoundedDeliveryText::new(format!("terminal-{index}"), 128)?;
        assert_eq!(
            database
                .advance_checkpoint(CheckpointAdvance::new(
                    terminal_worker,
                    None,
                    CommitCursor::try_from(u64::try_from(index)? + 1)?,
                    terminal_id,
                ))
                .await?,
            CheckpointAdvanceOutcome::Advanced
        );
    }

    database.close().await?;
    database.reopen().await?;
    let checkpoint = database
        .checkpoint(CheckpointQuery::new(worker))
        .await?
        .ok_or("checkpoint missing after restart")?;
    assert_eq!(checkpoint.cursor(), CommitCursor::try_from(5)?);
    database.close().await?;
    Ok(())
}
