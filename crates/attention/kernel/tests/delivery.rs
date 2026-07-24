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
