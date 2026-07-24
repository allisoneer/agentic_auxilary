mod support;

use attention_kernel::*;
use chrono::Utc;
use futures::executor::block_on;
use support::MemoryAdapter;

#[test]
fn exact_replay_returns_original_metadata_without_duplicate_effects() {
    let adapter = MemoryAdapter::new();
    let key = MutationIdempotencyKey::new();
    let command = CreateWorkItem::new(WorkItemId::new(), None, None, None, None, key);
    let first_bundle = evaluate_create_work_item(
        &command,
        EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
    );
    let first = block_on(adapter.commit_create_work_item(first_bundle)).expect("first commit");
    let replay_bundle = evaluate_create_work_item(
        &command,
        EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
    );
    let replay = block_on(adapter.commit_create_work_item(replay_bundle)).expect("exact replay");
    assert_eq!(first.disposition(), CommandDisposition::Applied);
    assert_eq!(replay.disposition(), CommandDisposition::Replayed);
    assert_eq!(first.value(), replay.value());
    assert_eq!(first.cursor(), replay.cursor());
    assert_eq!(first.change_event_id(), replay.change_event_id());
    assert_eq!(first.outbox_intent_id(), replay.outbox_intent_id());
    assert_eq!(adapter.event_count(), 1);
    assert_eq!(adapter.inbox().len(), 1);

    let mismatch = CreateWorkItem::new(command.id(), Some(Utc::now()), None, None, None, key);
    let mismatch_bundle = evaluate_create_work_item(
        &mismatch,
        EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
    );
    assert!(matches!(
        block_on(adapter.commit_create_work_item(mismatch_bundle)),
        Err(PortError::Semantic(SemanticError::IdempotencyMismatch(_)))
    ));
    assert_eq!(adapter.event_count(), 1);
}
