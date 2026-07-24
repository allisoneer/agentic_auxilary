mod support;

use attention_kernel::*;
use chrono::DateTime;
use chrono::Utc;
use futures::executor::block_on;
use support::MemoryAdapter;

#[expect(clippy::expect_used, reason = "fixed delivery fixtures")]
fn at(hour: u32) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&format!("2026-07-24T{hour:02}:00:00Z"))
        .expect("timestamp")
        .with_timezone(&Utc)
}

fn intent(id: OutboxIntentId, created_at: DateTime<Utc>) -> OutboxIntent {
    let signal_id = AttentionSignalId::new();
    OutboxIntent::new(
        id,
        OutboxDeduplicationKey::for_attention_signal(signal_id),
        DeliverySubject::AttentionSignal(signal_id),
        ChangeEventId::new(),
        created_at,
        DeliveryPurpose::FreshAttention,
    )
}

#[test]
fn active_lease_is_excluded_until_its_exact_expiry() {
    let adapter = MemoryAdapter::new();
    let intent_id = OutboxIntentId::new();
    adapter.insert_delivery(intent(intent_id, at(10)));

    let first = block_on(adapter.claim(DeliveryClaimQuery::new(
        at(12),
        at(13),
        ClaimLimit::try_from(1).expect("claim limit"),
    )))
    .expect("initial claim")[0];
    assert!(
        block_on(adapter.claim(DeliveryClaimQuery::new(
            at(12),
            at(14),
            ClaimLimit::try_from(1).expect("claim limit"),
        )))
        .expect("active lease query")
        .is_empty()
    );
    let authority = block_on(adapter.inspect(intent_id))
        .expect("inspect active lease")
        .expect("delivery authority");
    assert!(matches!(
        authority.state().status(),
        DeliveryStatus::Leased { token, expires_at }
            if *token == first.token() && *expires_at == at(13)
    ));

    let reclaimed = block_on(adapter.claim(DeliveryClaimQuery::new(
        at(13),
        at(14),
        ClaimLimit::try_from(1).expect("claim limit"),
    )))
    .expect("exact-expiry claim");
    assert_eq!(reclaimed.len(), 1);
    assert_ne!(reclaimed[0].token(), first.token());
}

#[test]
fn retryable_delivery_is_excluded_until_its_exact_retry_time() {
    let adapter = MemoryAdapter::new();
    let intent_id = OutboxIntentId::new();
    adapter.insert_delivery(intent(intent_id, at(10)));
    let first = block_on(adapter.claim(DeliveryClaimQuery::new(
        at(12),
        at(13),
        ClaimLimit::try_from(1).expect("claim limit"),
    )))
    .expect("initial claim")[0];
    assert_eq!(
        block_on(adapter.fail_retryable(
            intent_id,
            first.token(),
            1,
            BoundedDeliveryText::new("temporary", 16).expect("retry error"),
            at(15),
        ))
        .expect("retryable outcome"),
        DeliveryCompletionOutcome::Applied
    );
    assert!(
        block_on(adapter.claim(DeliveryClaimQuery::new(
            at(14),
            at(16),
            ClaimLimit::try_from(1).expect("claim limit"),
        )))
        .expect("early retry query")
        .is_empty()
    );

    let retried = block_on(adapter.claim(DeliveryClaimQuery::new(
        at(15),
        at(16),
        ClaimLimit::try_from(1).expect("claim limit"),
    )))
    .expect("exact retry-time claim");
    assert_eq!(retried.len(), 1);
    assert_eq!(retried[0].intent_id(), intent_id);
}

#[test]
fn delivery_claims_are_oldest_first_before_applying_limit() {
    let adapter = MemoryAdapter::new();
    let active_id = OutboxIntentId::new();
    adapter.insert_delivery(intent(active_id, at(8)));
    block_on(adapter.claim(DeliveryClaimQuery::new(
        at(12),
        at(20),
        ClaimLimit::try_from(1).expect("claim limit"),
    )))
    .expect("active claim");

    let oldest_id = OutboxIntentId::new();
    let same_time_ids = [OutboxIntentId::new(), OutboxIntentId::new()];
    let newest_id = OutboxIntentId::new();
    adapter.insert_delivery(intent(newest_id, at(11)));
    adapter.insert_delivery(intent(same_time_ids[1], at(10)));
    adapter.insert_delivery(intent(oldest_id, at(9)));
    adapter.insert_delivery(intent(same_time_ids[0], at(10)));

    let claims = block_on(adapter.claim(DeliveryClaimQuery::new(
        at(12),
        at(13),
        ClaimLimit::try_from(3).expect("claim limit"),
    )))
    .expect("ordered claims");
    let mut tie_ids = same_time_ids;
    tie_ids.sort_unstable();
    assert_eq!(
        claims
            .iter()
            .map(|claim| claim.intent_id())
            .collect::<Vec<_>>(),
        vec![oldest_id, tie_ids[0], tie_ids[1]]
    );
    assert!(!claims.iter().any(|claim| claim.intent_id() == active_id));
    assert!(!claims.iter().any(|claim| claim.intent_id() == newest_id));
}

#[test]
fn reclaim_fences_stale_worker_and_checkpoint_requires_terminal_state() {
    let adapter = MemoryAdapter::new();
    let intent = OutboxIntent::new(
        OutboxIntentId::new(),
        OutboxDeduplicationKey::for_attention_signal(AttentionSignalId::new()),
        DeliverySubject::AttentionSignal(AttentionSignalId::new()),
        ChangeEventId::new(),
        at(12),
        DeliveryPurpose::FreshAttention,
    );
    let intent_id = intent.id();
    adapter.insert_delivery(intent);
    let first = block_on(adapter.claim(DeliveryClaimQuery::new(
        at(12),
        at(13),
        ClaimLimit::try_from(1).expect("claim limit"),
    )))
    .expect("claim")[0];
    let worker = BoundedDeliveryText::new("worker", 16).expect("worker");
    let advance = CheckpointAdvance::new(
        worker,
        None,
        CommitCursor::try_from(2).expect("cursor"),
        intent_id,
    );
    assert_eq!(
        block_on(adapter.advance_checkpoint(advance.clone())).expect("checkpoint result"),
        CheckpointAdvanceOutcome::TerminalStateRequired
    );

    let reclaimed = block_on(adapter.claim(DeliveryClaimQuery::new(
        at(14),
        at(15),
        ClaimLimit::try_from(1).expect("claim limit"),
    )))
    .expect("reclaim")[0];
    assert_ne!(first.token(), reclaimed.token());
    assert_eq!(
        block_on(adapter.succeed(
            intent_id,
            first.token(),
            ProviderMessageId::new("stale", 16).expect("message ID"),
            at(14),
        ))
        .expect("stale completion outcome"),
        DeliveryCompletionOutcome::Fenced
    );
    assert_eq!(
        block_on(adapter.succeed(
            intent_id,
            reclaimed.token(),
            ProviderMessageId::new("message-1", 16).expect("message ID"),
            at(14),
        ))
        .expect("completion outcome"),
        DeliveryCompletionOutcome::Applied
    );
    assert_eq!(
        block_on(adapter.advance_checkpoint(advance)).expect("checkpoint result"),
        CheckpointAdvanceOutcome::Advanced
    );
    assert_eq!(adapter.event_count(), 0);
    assert_eq!(adapter.outbox_count(), 1);
}
