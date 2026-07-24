mod support;

use attention_kernel::AttentionCommitPort;
use attention_kernel::AttentionSignalId;
use attention_kernel::ChangeEventId;
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

    let newer = fixture(2, entity_id, signal_id);
    let advanced =
        evaluate_ingest_source_occurrence(&newer, first.entity(), first.signal(), context())
            .expect("newer source observation");
    assert_eq!(
        advanced.value().decision(),
        SourceIngestionDecision::Advanced
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
    assert!(receipt_only.effects().change().affected_views().is_empty());
    assert!(receipt_only.effects().change().inbox_effects().is_empty());
}

#[test]
fn occurrence_dedupe_and_source_version_races_are_transaction_local() {
    let adapter = MemoryAdapter::new();
    let entity_id = SourceEntityId::new();
    let signal_id = AttentionSignalId::new();
    let initial = fixture(1, entity_id, signal_id);
    let initial_bundle = evaluate_ingest_source_occurrence(&initial, None, None, context())
        .expect("evaluate initial");
    let initial_entity = initial_bundle.entity().cloned().expect("entity");
    let initial_signal = initial_bundle.signal().cloned().expect("signal");
    let first =
        block_on(adapter.commit_ingest_source_occurrence(initial_bundle)).expect("commit initial");

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
