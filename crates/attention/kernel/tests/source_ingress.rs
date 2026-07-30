mod support;

use attention_kernel::AffectedView;
use attention_kernel::AttentionCommitPort;
use attention_kernel::AttentionReadPort;
use attention_kernel::AttentionSignalId;
use attention_kernel::ChangeEventId;
use attention_kernel::ChangeKind;
use attention_kernel::CommandDisposition;
use attention_kernel::EvaluationContext;
use attention_kernel::ExternalEntityId;
use attention_kernel::IngestSourceOccurrence;
use attention_kernel::MutationIdempotencyKey;
use attention_kernel::NormalizedSourceOrder;
use attention_kernel::OccurrenceId;
use attention_kernel::OccurrenceKey;
use attention_kernel::PortError;
use attention_kernel::ReceiptOnlyReason;
use attention_kernel::SemanticError;
use attention_kernel::SignalSourceLifecycle;
use attention_kernel::SourceComparatorDomain;
use attention_kernel::SourceEntityId;
use attention_kernel::SourceEntityIdentity;
use attention_kernel::SourceEntityKey;
use attention_kernel::SourceIngestionDecision;
use attention_kernel::SourceInstance;
use attention_kernel::SourceKind;
use attention_kernel::SourceOrderMode;
use attention_kernel::SourceReceiptId;
use attention_kernel::evaluate_ingest_source_occurrence;
use chrono::DateTime;
use chrono::Utc;
use futures::executor::block_on;
use support::MemoryAdapter;

#[expect(clippy::expect_used, reason = "fixed source fixtures")]
fn fixture(
    order: u8,
    entity_id: SourceEntityId,
    signal_id: AttentionSignalId,
) -> IngestSourceOccurrence {
    let kind = SourceKind::new("linear").expect("kind");
    let instance = SourceInstance::new("workspace").expect("instance");
    let occurred_at = DateTime::parse_from_rfc3339("2026-07-24T12:00:00Z")
        .expect("timestamp")
        .with_timezone(&Utc);
    IngestSourceOccurrence::new(
        SourceReceiptId::new(),
        Some(SourceEntityIdentity::new(
            entity_id,
            SourceEntityKey::new(
                kind.clone(),
                instance.clone(),
                ExternalEntityId::new("ENG-1118").expect("entity"),
            ),
        )),
        signal_id,
        OccurrenceKey::new(
            kind,
            instance,
            OccurrenceId::new(format!("event-{order}")).expect("occurrence"),
        ),
        occurred_at,
        occurred_at,
        SourceOrderMode::Ordered {
            domain: SourceComparatorDomain::new("sequence").expect("domain"),
            value: Some(NormalizedSourceOrder::new([order]).expect("order")),
        },
        SignalSourceLifecycle::Active,
        false,
        MutationIdempotencyKey::new(),
    )
}

fn context() -> EvaluationContext {
    EvaluationContext::new(ChangeEventId::new(), None, Utc::now())
}

fn entity_free_fixture(order: u8, signal_id: AttentionSignalId) -> IngestSourceOccurrence {
    entity_free_fixture_with_receipt(order, signal_id, SourceReceiptId::new())
}

#[expect(clippy::expect_used, reason = "validated entity-free source fixture")]
fn entity_free_fixture_with_receipt(
    order: u8,
    signal_id: AttentionSignalId,
    receipt_id: SourceReceiptId,
) -> IngestSourceOccurrence {
    let occurred_at = DateTime::parse_from_rfc3339("2026-07-24T12:00:00Z")
        .expect("timestamp")
        .with_timezone(&Utc);
    IngestSourceOccurrence::new(
        receipt_id,
        None,
        signal_id,
        OccurrenceKey::new(
            SourceKind::new("linear").expect("kind"),
            SourceInstance::new("workspace").expect("instance"),
            OccurrenceId::new(format!("entity-free-{order}")).expect("occurrence"),
        ),
        occurred_at,
        occurred_at,
        SourceOrderMode::Ordered {
            domain: SourceComparatorDomain::new("sequence").expect("domain"),
            value: Some(NormalizedSourceOrder::new([order]).expect("order")),
        },
        SignalSourceLifecycle::Active,
        false,
        MutationIdempotencyKey::new(),
    )
}

#[expect(clippy::expect_used, reason = "validated source retry fixtures")]
fn retry(command: &IngestSourceOccurrence, order: u8) -> IngestSourceOccurrence {
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

#[test]
fn newer_source_advances_and_equal_source_is_receipt_only() {
    let entity_id = SourceEntityId::new();
    let signal_id = AttentionSignalId::new();
    let initial = fixture(1, entity_id, signal_id);
    let first = evaluate_ingest_source_occurrence(&initial, None, None, context())
        .expect("initial source observation");
    assert_eq!(first.value().decision(), SourceIngestionDecision::Advanced);
    assert_eq!(
        first.effects().change().kind(),
        ChangeKind::SourceOccurrenceIngested
    );
    assert_eq!(
        first.effects().change().affected_views(),
        &[
            AffectedView::SourceReceipt {
                source_receipt: first.receipt().clone(),
            },
            AffectedView::SourceEntity {
                source_entity: first.entity().expect("initial entity").clone(),
            },
            AffectedView::AttentionSignal {
                attention_signal: first.signal().expect("initial signal").clone(),
            },
        ]
    );

    let newer = fixture(2, entity_id, signal_id);
    let advanced =
        evaluate_ingest_source_occurrence(&newer, first.entity(), first.signal(), context())
            .expect("newer source observation");
    assert_eq!(
        advanced.value().decision(),
        SourceIngestionDecision::Advanced
    );
    assert_eq!(
        advanced.effects().change().affected_views(),
        &[
            AffectedView::SourceReceipt {
                source_receipt: advanced.receipt().clone(),
            },
            AffectedView::SourceEntity {
                source_entity: advanced.entity().expect("advanced entity").clone(),
            },
            AffectedView::AttentionSignal {
                attention_signal: advanced.signal().expect("advanced signal").clone(),
            },
        ]
    );

    let equal = fixture(2, entity_id, signal_id);
    let receipt_only =
        evaluate_ingest_source_occurrence(&equal, advanced.entity(), advanced.signal(), context())
            .expect("equal unique occurrence");
    assert_eq!(
        receipt_only.value().decision(),
        SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Equal)
    );
    assert!(receipt_only.signal().is_none());
    assert_eq!(
        receipt_only.effects().change().affected_views(),
        &[AffectedView::SourceReceipt {
            source_receipt: receipt_only.receipt().clone(),
        }]
    );
    assert!(receipt_only.effects().change().inbox_effects().is_empty());
}

#[test]
fn entity_free_advanced_source_is_receipt_then_signal() {
    let command = entity_free_fixture(1, AttentionSignalId::new());
    let bundle = evaluate_ingest_source_occurrence(&command, None, None, context())
        .expect("entity-free source observation");
    assert_eq!(bundle.value().decision(), SourceIngestionDecision::Advanced);
    assert!(bundle.entity().is_none());
    assert_eq!(
        bundle.effects().change().affected_views(),
        &[
            AffectedView::SourceReceipt {
                source_receipt: bundle.receipt().clone(),
            },
            AffectedView::AttentionSignal {
                attention_signal: bundle.signal().expect("entity-free signal").clone(),
            },
        ]
    );
}

#[test]
fn occurrence_dedupe_and_source_version_races_are_transaction_local() {
    let adapter = MemoryAdapter::new();
    let entity_id = SourceEntityId::new();
    let signal_id = AttentionSignalId::new();
    let initial = fixture(1, entity_id, signal_id);
    let initial_bundle = evaluate_ingest_source_occurrence(&initial, None, None, context())
        .expect("evaluate initial");
    let initial_receipt = initial_bundle.receipt().clone();
    let initial_entity = initial_bundle.entity().cloned().expect("entity");
    let initial_signal = initial_bundle.signal().cloned().expect("signal");
    let first =
        block_on(adapter.commit_ingest_source_occurrence(initial_bundle)).expect("commit initial");
    assert_eq!(
        block_on(adapter.source_receipt(initial_receipt.id())).expect("read known receipt"),
        Some(initial_receipt)
    );
    assert_eq!(
        block_on(adapter.source_receipt(SourceReceiptId::new())).expect("read unknown receipt"),
        None
    );

    let exact = retry(&initial, 1);
    let exact_bundle = evaluate_ingest_source_occurrence(
        &exact,
        Some(&initial_entity),
        Some(&initial_signal),
        context(),
    )
    .expect("evaluate exact duplicate");
    let replay =
        block_on(adapter.commit_ingest_source_occurrence(exact_bundle)).expect("occurrence replay");
    assert_eq!(first.disposition(), CommandDisposition::Applied);
    assert_eq!(replay.disposition(), CommandDisposition::Replayed);
    assert_eq!(first.value(), replay.value());
    assert_eq!(first.cursor(), replay.cursor());
    assert_eq!(adapter.event_count(), 1);

    let changed = retry(&initial, 2);
    let changed_bundle = evaluate_ingest_source_occurrence(
        &changed,
        Some(&initial_entity),
        Some(&initial_signal),
        context(),
    )
    .expect("evaluate changed duplicate");
    assert!(matches!(
        block_on(adapter.commit_ingest_source_occurrence(changed_bundle)),
        Err(PortError::Semantic(
            SemanticError::OccurrenceContentMismatch(_)
        ))
    ));

    let newer = fixture(2, entity_id, signal_id);
    let newest = fixture(3, entity_id, signal_id);
    let newer_bundle = evaluate_ingest_source_occurrence(
        &newer,
        Some(&initial_entity),
        Some(&initial_signal),
        context(),
    )
    .expect("evaluate newer");
    let newest_bundle = evaluate_ingest_source_occurrence(
        &newest,
        Some(&initial_entity),
        Some(&initial_signal),
        context(),
    )
    .expect("evaluate concurrent newest");
    block_on(adapter.commit_ingest_source_occurrence(newer_bundle)).expect("version winner");
    assert!(matches!(
        block_on(adapter.commit_ingest_source_occurrence(newest_bundle)),
        Err(PortError::Semantic(
            SemanticError::ObservedSourceVersionConflict { .. }
        ))
    ));
}

#[test]
fn receipt_id_collision_is_pre_mutation_and_does_not_poison_adapter() {
    let adapter = MemoryAdapter::new();
    let initial = entity_free_fixture(1, AttentionSignalId::new());
    let receipt_id = initial.receipt_id();
    let initial_bundle = evaluate_ingest_source_occurrence(&initial, None, None, context())
        .expect("evaluate initial receipt");
    let initial_receipt = initial_bundle.receipt().clone();
    block_on(adapter.commit_ingest_source_occurrence(initial_bundle)).expect("commit initial");

    let colliding = entity_free_fixture_with_receipt(2, AttentionSignalId::new(), receipt_id);
    let colliding_key = colliding.occurrence_key().clone();
    let colliding_bundle = evaluate_ingest_source_occurrence(&colliding, None, None, context())
        .expect("evaluate colliding receipt");
    let unknown_id = SourceReceiptId::new();
    let before_event_count = adapter.event_count();
    let before_cursor = block_on(adapter.snapshot())
        .expect("snapshot before")
        .cursor();
    let before_inbox = adapter.inbox();
    let before_known = block_on(adapter.source_receipt(receipt_id)).expect("known before");
    let before_unknown = block_on(adapter.source_receipt(unknown_id)).expect("unknown before");

    let collision = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        block_on(adapter.commit_ingest_source_occurrence(colliding_bundle))
    }));
    let panic = collision.expect_err("receipt ID collision must panic");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("string panic message");
    assert!(message.contains("source receipt ID index collision"));

    assert_eq!(adapter.event_count(), before_event_count);
    assert_eq!(
        block_on(adapter.snapshot())
            .expect("snapshot after")
            .cursor(),
        before_cursor
    );
    assert_eq!(adapter.inbox(), before_inbox);
    assert_eq!(
        block_on(adapter.source_receipt(receipt_id)).expect("known after"),
        before_known
    );
    assert_eq!(before_known, Some(initial_receipt));
    assert_eq!(adapter.receipt_for_occurrence(&colliding_key), None);
    assert_eq!(
        block_on(adapter.source_receipt(unknown_id)).expect("unknown after"),
        before_unknown
    );
    assert_eq!(before_unknown, None);

    let later = entity_free_fixture(3, AttentionSignalId::new());
    let later_receipt_id = later.receipt_id();
    let later_bundle = evaluate_ingest_source_occurrence(&later, None, None, context())
        .expect("evaluate later valid receipt");
    block_on(adapter.commit_ingest_source_occurrence(later_bundle)).expect("later valid commit");
    assert_eq!(adapter.event_count(), before_event_count + 1);
    assert!(
        block_on(adapter.source_receipt(later_receipt_id))
            .expect("later receipt read")
            .is_some()
    );
}
