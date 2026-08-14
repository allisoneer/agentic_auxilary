use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use chrono::DateTime;
use chrono::Utc;
use std::collections::HashSet;
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn at(value: &str) -> TestResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

async fn database() -> TestResult<(tempfile::TempDir, AttentionDatabase)> {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config).await?;
    database.run_startup_migrations().await?;
    Ok((root, database))
}

async fn pending(
    database: &AttentionDatabase,
    sequence: usize,
    created_at: DateTime<Utc>,
) -> TestResult<OutboxIntentId> {
    let intent_id = OutboxIntentId::new();
    let command = IngestSourceOccurrence::new(
        SourceReceiptId::new(),
        None,
        AttentionSignalId::new(),
        OccurrenceKey::new(
            SourceKind::new("delivery-test")?,
            SourceInstance::new(format!("worker-{sequence}"))?,
            OccurrenceId::new(format!("occurrence-{sequence}"))?,
        ),
        created_at,
        created_at,
        SourceOrderMode::Unordered,
        SignalSourceLifecycle::Active,
        true,
        MutationIdempotencyKey::new(),
    );
    let outcome = database
        .commit_ingest_source_occurrence(evaluate_ingest_source_occurrence(
            &command,
            None,
            None,
            EvaluationContext::new(ChangeEventId::new(), Some(intent_id), created_at),
        )?)
        .await?;
    assert_eq!(outcome.outbox_intent_id(), Some(intent_id));
    Ok(intent_id)
}

fn claim_query(
    eligible_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    limit: usize,
) -> TestResult<DeliveryClaimQuery> {
    Ok(DeliveryClaimQuery::new(
        eligible_at,
        expires_at,
        ClaimLimit::try_from(limit)?,
    ))
}

#[tokio::test]
async fn inspect_claim_order_boundaries_reclaim_renew_and_missing_outcomes() -> TestResult {
    let (_root, database) = database().await?;
    let fractional = pending(&database, 1, at("2026-08-03T12:00:00.5Z")?).await?;
    let whole = pending(&database, 2, at("2026-08-03T12:00:00Z")?).await?;
    assert!(matches!(
        database
            .inspect(whole)
            .await?
            .expect("authority")
            .state()
            .status(),
        DeliveryStatus::Pending
    ));
    assert!(database.inspect(OutboxIntentId::new()).await?.is_none());

    let first = database
        .claim(claim_query(
            at("2026-08-03T12:01:00Z")?,
            at("2026-08-03T12:02:00Z")?,
            1,
        )?)
        .await?;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].intent_id(), whole);
    let original_token = first[0].token();
    assert_eq!(
        database
            .renew(
                whole,
                DeliveryLeaseToken::from_bytes([9; 32]),
                at("2026-08-03T12:03:00Z")?
            )
            .await?,
        RenewOutcome::Fenced
    );
    assert_eq!(
        database
            .renew(whole, original_token, at("2026-08-03T12:03:00Z")?)
            .await?,
        RenewOutcome::Renewed
    );
    assert_eq!(
        database
            .renew(
                OutboxIntentId::new(),
                original_token,
                at("2026-08-03T12:03:00Z")?
            )
            .await?,
        RenewOutcome::NotLeased
    );

    let second = database
        .claim(claim_query(
            at("2026-08-03T12:03:00Z")?,
            at("2026-08-03T12:04:00Z")?,
            2,
        )?)
        .await?;
    assert_eq!(second.len(), 2);
    assert_eq!(second[0].intent_id(), whole);
    assert_eq!(second[1].intent_id(), fractional);
    assert_ne!(second[0].token(), original_token);
    database.close().await?;
    Ok(())
}

#[tokio::test]
async fn completion_precedence_retry_eligibility_and_event_independence() -> TestResult {
    let (_root, database) = database().await?;
    let created_at = at("2026-08-03T12:00:00Z")?;
    let ids = [
        pending(&database, 10, created_at).await?,
        pending(&database, 11, created_at).await?,
        pending(&database, 12, created_at).await?,
        pending(&database, 13, created_at).await?,
    ];
    let before = database.snapshot().await?.cursor();
    let claims = database
        .claim(claim_query(
            at("2026-08-03T12:01:00Z")?,
            at("2026-08-03T12:02:00Z")?,
            8,
        )?)
        .await?;
    let claim = |id| {
        claims
            .iter()
            .copied()
            .find(|claim| claim.intent_id() == id)
            .ok_or("claim missing")
    };

    let success = claim(ids[0])?;
    let provider = ProviderMessageId::new("provider-1", 128)?;
    let succeeded_at = at("2026-08-03T12:01:30Z")?;
    assert_eq!(
        database
            .succeed(ids[0], success.token(), provider.clone(), succeeded_at)
            .await?,
        DeliveryCompletionOutcome::Applied
    );
    let stale = DeliveryLeaseToken::from_bytes([7; 32]);
    assert_eq!(
        database
            .succeed(ids[0], stale, provider.clone(), succeeded_at)
            .await?,
        DeliveryCompletionOutcome::Fenced
    );
    assert_eq!(
        database
            .succeed(ids[0], success.token(), provider.clone(), succeeded_at)
            .await?,
        DeliveryCompletionOutcome::Repeated
    );
    assert_eq!(
        database
            .succeed(
                ids[0],
                stale,
                ProviderMessageId::new("provider-2", 128)?,
                succeeded_at
            )
            .await?,
        DeliveryCompletionOutcome::Fenced
    );
    assert_eq!(
        database
            .succeed(
                ids[0],
                success.token(),
                ProviderMessageId::new("provider-2", 128)?,
                succeeded_at
            )
            .await?,
        DeliveryCompletionOutcome::Conflict
    );

    let skipped = claim(ids[1])?;
    let reason = BoundedDeliveryText::new("not applicable", 128)?;
    let skipped_at = at("2026-08-03T12:01:31Z")?;
    assert_eq!(
        database
            .skip(ids[1], skipped.token(), reason.clone(), skipped_at)
            .await?,
        DeliveryCompletionOutcome::Applied
    );
    assert_eq!(
        database
            .skip(ids[1], stale, reason.clone(), skipped_at)
            .await?,
        DeliveryCompletionOutcome::Fenced
    );
    assert_eq!(
        database
            .skip(ids[1], skipped.token(), reason.clone(), skipped_at)
            .await?,
        DeliveryCompletionOutcome::Repeated
    );
    assert_eq!(
        database
            .skip(
                ids[1],
                skipped.token(),
                BoundedDeliveryText::new("changed", 128)?,
                skipped_at
            )
            .await?,
        DeliveryCompletionOutcome::Conflict
    );

    let terminal = claim(ids[2])?;
    let terminal_error = BoundedDeliveryText::new("terminal", 128)?;
    let failed_at = at("2026-08-03T12:01:32Z")?;
    assert_eq!(
        database
            .fail_terminal(
                ids[2],
                terminal.token(),
                u32::MAX,
                terminal_error.clone(),
                failed_at
            )
            .await?,
        DeliveryCompletionOutcome::Applied
    );
    assert_eq!(
        database
            .fail_terminal(ids[2], stale, u32::MAX, terminal_error.clone(), failed_at)
            .await?,
        DeliveryCompletionOutcome::Fenced
    );
    assert_eq!(
        database
            .fail_terminal(
                ids[2],
                terminal.token(),
                u32::MAX,
                terminal_error,
                failed_at
            )
            .await?,
        DeliveryCompletionOutcome::Repeated
    );

    let retry = claim(ids[3])?;
    let retry_error = BoundedDeliveryText::new("retry", 128)?;
    let retry_at = at("2026-08-03T12:05:00Z")?;
    assert_eq!(
        database
            .fail_retryable(ids[3], retry.token(), 2, retry_error, retry_at)
            .await?,
        DeliveryCompletionOutcome::Applied
    );
    assert_eq!(
        database
            .fail_retryable(
                ids[3],
                stale,
                2,
                BoundedDeliveryText::new("retry", 128)?,
                retry_at
            )
            .await?,
        DeliveryCompletionOutcome::Fenced
    );
    assert_eq!(
        database
            .fail_retryable(
                ids[3],
                retry.token(),
                2,
                BoundedDeliveryText::new("retry", 128)?,
                retry_at
            )
            .await?,
        DeliveryCompletionOutcome::Repeated
    );
    assert!(
        database
            .claim(claim_query(
                at("2026-08-03T12:04:59Z")?,
                at("2026-08-03T12:06:00Z")?,
                8
            )?)
            .await?
            .is_empty()
    );
    let due_retry = database
        .claim(claim_query(retry_at, at("2026-08-03T12:06:00Z")?, 8)?)
        .await?;
    assert_eq!(due_retry.len(), 1);
    assert_eq!(due_retry[0].intent_id(), ids[3]);

    assert_eq!(database.snapshot().await?.cursor(), before);
    assert_eq!(
        database
            .succeed(
                OutboxIntentId::new(),
                stale,
                ProviderMessageId::new("missing", 128)?,
                succeeded_at
            )
            .await?,
        DeliveryCompletionOutcome::Fenced
    );
    database.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_claims_are_disjoint() -> TestResult {
    let (_root, database) = database().await?;
    let time = at("2026-08-03T12:00:00Z")?;
    for index in 20..24 {
        pending(&database, index, time).await?;
    }
    let query = claim_query(at("2026-08-03T12:01:00Z")?, at("2026-08-03T12:02:00Z")?, 2)?;
    let first_database = database.clone();
    let first = tokio::spawn(async move { first_database.claim(query).await });
    let second_database = database.clone();
    let second = tokio::spawn(async move { second_database.claim(query).await });
    let first = first.await??;
    let second = second.await??;
    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 2);
    let first_ids = first
        .iter()
        .map(|claim| claim.intent_id())
        .collect::<HashSet<_>>();
    assert!(
        second
            .iter()
            .all(|claim| !first_ids.contains(&claim.intent_id()))
    );
    database.close().await?;
    Ok(())
}
