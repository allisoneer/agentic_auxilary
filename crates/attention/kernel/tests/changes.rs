mod support;

use attention_kernel::*;
use chrono::DateTime;
use chrono::Utc;
use futures::executor::block_on;
use support::MemoryAdapter;

#[expect(clippy::expect_used, reason = "fixed replay fixtures")]
fn at(hour: u32) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&format!("2026-07-24T{hour:02}:00:00Z"))
        .expect("timestamp")
        .with_timezone(&Utc)
}

fn context() -> EvaluationContext {
    EvaluationContext::new(ChangeEventId::new(), None, at(12))
}

#[expect(clippy::expect_used, reason = "bounded replay query fixture")]
fn first_change_after(adapter: &MemoryAdapter, cursor: CommitCursor) -> ChangeEvent {
    let changes = block_on(adapter.changes_after(ChangesAfterQuery::new(
        cursor,
        QueryLimit::try_from(1).expect("limit"),
    )))
    .expect("changes");
    let ChangesResult::Page(page) = changes else {
        panic!("expected page");
    };
    page.events().first().expect("one change").clone()
}

#[expect(clippy::expect_used, reason = "validated source replay fixtures")]
fn source_command(
    order: u8,
    entity_id: SourceEntityId,
    signal_id: AttentionSignalId,
) -> IngestSourceOccurrence {
    let kind = SourceKind::new("linear").expect("kind");
    let instance = SourceInstance::new("workspace").expect("instance");
    IngestSourceOccurrence::new(
        SourceReceiptId::new(),
        Some(SourceEntityIdentity::new(
            entity_id,
            SourceEntityKey::new(
                kind.clone(),
                instance.clone(),
                ExternalEntityId::new("ENG-1150").expect("entity"),
            ),
        )),
        signal_id,
        OccurrenceKey::new(
            kind,
            instance,
            OccurrenceId::new(format!("replay-{order}")).expect("occurrence"),
        ),
        at(12),
        at(12),
        SourceOrderMode::Ordered {
            domain: SourceComparatorDomain::new("sequence").expect("domain"),
            value: Some(NormalizedSourceOrder::new([order]).expect("order")),
        },
        SignalSourceLifecycle::Active,
        false,
        MutationIdempotencyKey::new(),
    )
}

#[test]
fn snapshots_pages_and_gaps_preserve_cursor_contracts() {
    let adapter = MemoryAdapter::new();
    let initial = block_on(adapter.snapshot()).expect("initial snapshot");
    for _ in 0..3 {
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
        block_on(adapter.commit_create_work_item(bundle)).expect("create");
    }
    let page = block_on(adapter.changes_after(ChangesAfterQuery::new(
        initial.cursor(),
        QueryLimit::try_from(2).expect("limit"),
    )))
    .expect("changes");
    let ChangesResult::Page(page) = page else {
        panic!("expected page");
    };
    assert_eq!(page.events().len(), 2);
    assert!(page.has_more());
    assert!(page.events()[0].cursor() < page.events()[1].cursor());

    adapter.set_retention_floor(page.resume_after());
    assert!(matches!(
        block_on(adapter.changes_after(ChangesAfterQuery::new(
            initial.cursor(),
            QueryLimit::try_from(2).expect("limit"),
        )))
        .expect("gap result"),
        ChangesResult::Gap(_)
    ));
}

#[test]
fn work_item_history_retains_original_open_snapshot_after_completion() {
    let adapter = MemoryAdapter::new();
    let base = block_on(adapter.snapshot())
        .expect("base snapshot")
        .cursor();
    let command = CreateWorkItem::new(
        WorkItemId::new(),
        Some(at(13)),
        None,
        None,
        None,
        MutationIdempotencyKey::new(),
    );
    let create_bundle = evaluate_create_work_item(&command, context());
    let expected_created = create_bundle.root().clone();
    block_on(adapter.commit_create_work_item(create_bundle)).expect("create work item");
    let original = first_change_after(&adapter, base);
    assert_eq!(
        original.draft().affected_views(),
        &[AffectedView::WorkItem {
            work_item: expected_created.clone(),
        }]
    );

    let current = block_on(adapter.work_item(command.id()))
        .expect("read work item")
        .expect("work item");
    let complete = CompleteWorkItem::new(
        current.id(),
        current.revision(),
        MutationIdempotencyKey::new(),
    );
    let complete_bundle =
        evaluate_complete_work_item(&complete, &current, context()).expect("complete evaluation");
    block_on(adapter.commit_complete_work_item(complete_bundle)).expect("complete work item");

    assert_eq!(first_change_after(&adapter, base), original);
    assert_eq!(expected_created.lifecycle(), WorkItemLifecycle::Open);
    assert_eq!(expected_created.revision(), Revision::initial());
}

#[test]
fn source_history_retains_initial_receipt_entity_and_signal_after_later_changes() {
    let adapter = MemoryAdapter::new();
    let base = block_on(adapter.snapshot())
        .expect("base snapshot")
        .cursor();
    let entity_id = SourceEntityId::new();
    let signal_id = AttentionSignalId::new();
    let initial = source_command(1, entity_id, signal_id);
    let initial_bundle = evaluate_ingest_source_occurrence(&initial, None, None, context())
        .expect("initial evaluation");
    let expected_views = vec![
        AffectedView::SourceReceipt {
            source_receipt: initial_bundle.receipt().clone(),
        },
        AffectedView::SourceEntity {
            source_entity: initial_bundle.entity().expect("entity").clone(),
        },
        AffectedView::AttentionSignal {
            attention_signal: initial_bundle.signal().expect("signal").clone(),
        },
    ];
    block_on(adapter.commit_ingest_source_occurrence(initial_bundle)).expect("initial commit");
    let original = first_change_after(&adapter, base);
    assert_eq!(original.draft().affected_views(), expected_views);

    let entity_key = initial.entity().expect("entity identity").key().clone();
    let current_entity = block_on(adapter.source_entity(SourceAuthorityQuery::new(entity_key)))
        .expect("read entity")
        .expect("entity");
    let current_signal = block_on(adapter.attention_signal(signal_id))
        .expect("read signal")
        .expect("signal");
    let newer = source_command(2, entity_id, signal_id);
    let newer_bundle = evaluate_ingest_source_occurrence(
        &newer,
        Some(&current_entity),
        Some(&current_signal),
        context(),
    )
    .expect("newer evaluation");
    block_on(adapter.commit_ingest_source_occurrence(newer_bundle)).expect("newer commit");

    let newest_signal = block_on(adapter.attention_signal(signal_id))
        .expect("read newest signal")
        .expect("newest signal");
    let acknowledge = AcknowledgeAttentionSignal::new(
        signal_id,
        newest_signal.revision(),
        MutationIdempotencyKey::new(),
    );
    let acknowledge_bundle =
        evaluate_acknowledge_attention_signal(&acknowledge, &newest_signal, context())
            .expect("acknowledge evaluation");
    block_on(adapter.commit_acknowledge_attention_signal(acknowledge_bundle))
        .expect("acknowledge signal");

    assert_eq!(first_change_after(&adapter, base), original);
}

#[test]
fn reminder_history_retains_fired_snapshot_after_acknowledgement() {
    let adapter = MemoryAdapter::new();
    let reminder_id = ReminderId::new();
    let fire_id = ReminderFireId::new();
    let create_bundle = evaluate_create_reminder(
        &CreateReminder::new(
            reminder_id,
            fire_id,
            ReminderTarget::WorkItem(WorkItemId::new()),
            at(12),
            MutationIdempotencyKey::new(),
        ),
        context(),
    );
    let created = block_on(adapter.commit_create_reminder(create_bundle)).expect("create reminder");
    let current = block_on(adapter.reminder(reminder_id))
        .expect("read reminder")
        .expect("reminder");
    let fire_bundle = evaluate_fire_reminder(
        &FireReminder::new(reminder_id, fire_id, MutationIdempotencyKey::new()),
        &current,
        context(),
    )
    .expect("fire evaluation");
    let expected_fired = fire_bundle.root().clone();
    block_on(adapter.commit_fire_reminder(fire_bundle)).expect("fire reminder");
    let original = first_change_after(&adapter, created.cursor());
    assert_eq!(
        original.draft().affected_views(),
        &[AffectedView::Reminder {
            reminder: expected_fired.clone(),
        }]
    );

    let fired = block_on(adapter.reminder(reminder_id))
        .expect("read fired reminder")
        .expect("fired reminder");
    let acknowledge = AcknowledgeReminderFire::new(
        reminder_id,
        fire_id,
        fired.revision(),
        MutationIdempotencyKey::new(),
    );
    let acknowledge_bundle = evaluate_acknowledge_reminder_fire(&acknowledge, &fired, context())
        .expect("acknowledge evaluation");
    block_on(adapter.commit_acknowledge_reminder_fire(acknowledge_bundle))
        .expect("acknowledge reminder");

    assert_eq!(first_change_after(&adapter, created.cursor()), original);
    assert_eq!(expected_fired.fires()[0].state(), ReminderFireState::Fired);
}

#[test]
fn reminder_history_retains_fired_snapshot_and_snooze_chain() {
    let adapter = MemoryAdapter::new();
    let reminder_id = ReminderId::new();
    let fire_id = ReminderFireId::new();
    let create_bundle = evaluate_create_reminder(
        &CreateReminder::new(
            reminder_id,
            fire_id,
            ReminderTarget::WorkItem(WorkItemId::new()),
            at(12),
            MutationIdempotencyKey::new(),
        ),
        context(),
    );
    let created = block_on(adapter.commit_create_reminder(create_bundle)).expect("create reminder");
    let current = block_on(adapter.reminder(reminder_id))
        .expect("read reminder")
        .expect("reminder");
    let fire_bundle = evaluate_fire_reminder(
        &FireReminder::new(reminder_id, fire_id, MutationIdempotencyKey::new()),
        &current,
        context(),
    )
    .expect("fire evaluation");
    let expected_fired = fire_bundle.root().clone();
    let fired_outcome = block_on(adapter.commit_fire_reminder(fire_bundle)).expect("fire reminder");
    let original = first_change_after(&adapter, created.cursor());

    let fired = block_on(adapter.reminder(reminder_id))
        .expect("read fired reminder")
        .expect("fired reminder");
    let replacement_id = ReminderFireId::new();
    let snooze = SnoozeReminderFire::new(
        reminder_id,
        fire_id,
        replacement_id,
        at(13),
        fired.revision(),
        MutationIdempotencyKey::new(),
    );
    let snooze_bundle =
        evaluate_snooze_reminder_fire(&snooze, &fired, context()).expect("snooze evaluation");
    let expected_snoozed = snooze_bundle.root().clone();
    block_on(adapter.commit_snooze_reminder_fire(snooze_bundle)).expect("snooze reminder");

    assert_eq!(first_change_after(&adapter, created.cursor()), original);
    assert_eq!(expected_fired.fires()[0].state(), ReminderFireState::Fired);
    let snooze_event = first_change_after(&adapter, fired_outcome.cursor());
    assert_eq!(
        snooze_event.draft().affected_views(),
        &[AffectedView::Reminder {
            reminder: expected_snoozed.clone(),
        }]
    );
    assert_eq!(expected_snoozed.fires().len(), 2);
    assert_eq!(expected_snoozed.fires()[0].id(), fire_id);
    assert_eq!(
        expected_snoozed.fires()[0].state(),
        ReminderFireState::Snoozed
    );
    assert_eq!(expected_snoozed.fires()[1].id(), replacement_id);
    assert_eq!(
        expected_snoozed.fires()[1].state(),
        ReminderFireState::Scheduled
    );
}
