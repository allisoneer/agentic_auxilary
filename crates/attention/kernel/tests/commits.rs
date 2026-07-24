mod support;

use attention_kernel::*;
use chrono::Utc;
use futures::executor::block_on;
use support::MemoryAdapter;

fn context() -> EvaluationContext {
    EvaluationContext::new(ChangeEventId::new(), None, Utc::now())
}

#[test]
fn stale_revision_loser_commits_no_effects() {
    let adapter = MemoryAdapter::new();
    let create = CreateWorkItem::new(
        WorkItemId::new(),
        None,
        None,
        None,
        None,
        MutationIdempotencyKey::new(),
    );
    let created = evaluate_create_work_item(&create, context());
    block_on(adapter.commit_create_work_item(created)).expect("create");
    let current = block_on(adapter.work_item(create.id()))
        .expect("read")
        .expect("created root");
    let first = CompleteWorkItem::new(
        current.id(),
        current.revision(),
        MutationIdempotencyKey::new(),
    );
    let second = CompleteWorkItem::new(
        current.id(),
        current.revision(),
        MutationIdempotencyKey::new(),
    );
    let first_bundle = evaluate_complete_work_item(&first, &current, context()).expect("evaluate");
    let second_bundle =
        evaluate_complete_work_item(&second, &current, context()).expect("evaluate");
    block_on(adapter.commit_complete_work_item(first_bundle)).expect("winner");
    let effects_before_loser = (adapter.event_count(), adapter.inbox());
    assert!(matches!(
        block_on(adapter.commit_complete_work_item(second_bundle)),
        Err(PortError::Semantic(
            SemanticError::ExpectedRevisionConflict { .. }
        ))
    ));
    assert_eq!(
        effects_before_loser,
        (adapter.event_count(), adapter.inbox())
    );
}
