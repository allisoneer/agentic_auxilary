use attention_kernel::AcknowledgeAttentionSignal;
use attention_kernel::AttentionSignal;
use attention_kernel::AttentionSignalId;
use attention_kernel::CancelWorkItem;
use attention_kernel::CanonicalCommand;
use attention_kernel::ChangeEventId;
use attention_kernel::CompleteWorkItem;
use attention_kernel::CreateWorkItem;
use attention_kernel::EvaluationContext;
use attention_kernel::EvaluationError;
use attention_kernel::MutationIdempotencyKey;
use attention_kernel::OutboxIntentId;
use attention_kernel::Revision;
use attention_kernel::SourceReceiptId;
use attention_kernel::WorkItem;
use attention_kernel::WorkItemId;
use attention_kernel::WorkItemLifecycle;
use attention_kernel::evaluate_acknowledge_attention_signal;
use attention_kernel::evaluate_cancel_work_item;
use attention_kernel::evaluate_complete_work_item;
use attention_kernel::evaluate_create_work_item;
use chrono::DateTime;
use chrono::Utc;

#[expect(clippy::expect_used, reason = "fixed test timestamp")]
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-07-24T12:00:00Z")
        .expect("timestamp")
        .with_timezone(&Utc)
}

fn context() -> EvaluationContext {
    EvaluationContext::new(ChangeEventId::new(), None, now())
}

#[test]
fn work_item_evaluators_build_complete_root_event_and_inbox_effects() {
    let id = WorkItemId::new();
    let create = CreateWorkItem::new(id, None, None, None, None, MutationIdempotencyKey::new());
    let created = evaluate_create_work_item(&create, context());
    assert_eq!(created.root().revision(), Revision::initial());
    assert_eq!(created.effects().change().affected_views().len(), 1);
    assert_eq!(
        created.effects().change().inbox_effects().additions().len(),
        1
    );

    let complete = CompleteWorkItem::new(id, Revision::initial(), MutationIdempotencyKey::new());
    let completed = evaluate_complete_work_item(&complete, created.root(), context())
        .expect("complete open item");
    assert_eq!(completed.root().lifecycle(), WorkItemLifecycle::Completed);
    assert_eq!(
        completed
            .effects()
            .change()
            .inbox_effects()
            .removals()
            .len(),
        1
    );

    let stale = CompleteWorkItem::new(id, Revision::initial(), MutationIdempotencyKey::new());
    assert!(matches!(
        evaluate_complete_work_item(&stale, completed.root(), context()),
        Err(EvaluationError::Semantic(_))
    ));

    let separate = WorkItem::new(WorkItemId::new(), None, None, None, None);
    let cancel = CancelWorkItem::new(
        separate.id(),
        Revision::initial(),
        MutationIdempotencyKey::new(),
    );
    assert_eq!(
        evaluate_cancel_work_item(&cancel, &separate, context())
            .expect("cancel open item")
            .root()
            .lifecycle(),
        WorkItemLifecycle::Cancelled
    );
}

#[test]
fn signal_acknowledgement_is_revision_guarded_and_removes_inbox_entry() {
    let signal = AttentionSignal::new(AttentionSignalId::new(), SourceReceiptId::new(), None);
    let command = AcknowledgeAttentionSignal::new(
        signal.id(),
        signal.revision(),
        MutationIdempotencyKey::new(),
    );
    let bundle = evaluate_acknowledge_attention_signal(&command, &signal, context())
        .expect("acknowledge unread signal");
    assert_eq!(bundle.root().revision().value(), 2);
    assert_eq!(
        bundle.effects().change().inbox_effects().removals().len(),
        1
    );
}

#[test]
fn semantic_fields_change_fingerprints_but_generated_context_does_not_participate() {
    let id = WorkItemId::new();
    let key = MutationIdempotencyKey::new();
    let first = CreateWorkItem::new(id, None, None, None, None, key);
    let changed = CreateWorkItem::new(id, Some(now()), None, None, None, key);
    assert_ne!(
        first.canonical_fingerprint(),
        changed.canonical_fingerprint()
    );

    let first_bundle = evaluate_create_work_item(&first, context());
    let second_bundle = evaluate_create_work_item(
        &first,
        EvaluationContext::new(
            ChangeEventId::new(),
            Some(OutboxIntentId::new()),
            now() + chrono::TimeDelta::seconds(1),
        ),
    );
    assert_eq!(
        first_bundle.idempotency().fingerprint(),
        second_bundle.idempotency().fingerprint()
    );
}
