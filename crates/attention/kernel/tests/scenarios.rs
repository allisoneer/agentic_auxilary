//! T02-pure slices of accepted scenarios.
//!
//! Scenario 11 is non-applicable here because replay, persistence, protocol, server, and client
//! behavior belongs to later tickets. Scenario 12 is non-applicable because Outbox, external
//! delivery, checkpoints, and scheduling also belong to later tickets.

use attention_kernel::AttentionSignal;
use attention_kernel::AttentionSignalId;
use attention_kernel::ExternalEntityId;
use attention_kernel::OccurrenceId;
use attention_kernel::OccurrenceKey;
use attention_kernel::Reminder;
use attention_kernel::ReminderFireId;
use attention_kernel::ReminderFireState;
use attention_kernel::ReminderId;
use attention_kernel::ReminderTarget;
use attention_kernel::Revision;
use attention_kernel::SignalAttentionState;
use attention_kernel::SignalSourceLifecycle;
use attention_kernel::SourceEntity;
use attention_kernel::SourceEntityId;
use attention_kernel::SourceEntityKey;
use attention_kernel::SourceInstance;
use attention_kernel::SourceKind;
use attention_kernel::SourceReceipt;
use attention_kernel::SourceReceiptId;
use attention_kernel::WorkItem;
use attention_kernel::WorkItemId;
use attention_kernel::WorkItemLifecycle;
use chrono::DateTime;
use chrono::Utc;

#[expect(clippy::expect_used, reason = "fixed timestamp test fixture")]
fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("valid timestamp")
        .with_timezone(&Utc)
}

#[expect(clippy::expect_used, reason = "validated source-key test fixtures")]
fn source_keys(occurrence: &str) -> (OccurrenceKey, SourceEntityKey) {
    let kind = SourceKind::new("agentic").expect("source kind");
    let instance = SourceInstance::new("local").expect("source instance");
    (
        OccurrenceKey::new(
            kind.clone(),
            instance.clone(),
            OccurrenceId::new(occurrence).expect("occurrence ID"),
        ),
        SourceEntityKey::new(
            kind,
            instance,
            ExternalEntityId::new("run-42").expect("entity ID"),
        ),
    )
}

#[test]
fn scenario_01_open_manual_work_item_is_in_default_inbox() {
    let item = WorkItem::new(WorkItemId::new(), None, None, None, None);
    assert_eq!(item.lifecycle(), WorkItemLifecycle::Open);
    assert_eq!(item.revision(), Revision::initial());
    assert!(item.source_link().is_none());
    assert!(item.is_in_default_inbox());
}

#[test]
fn scenario_02_due_without_reminder_stays_open_and_has_no_implicit_reminder() {
    let due_at = at("2026-07-23T11:00:00Z");
    let item = WorkItem::new(WorkItemId::new(), Some(due_at), None, None, None);
    assert!(item.is_overdue(at("2026-07-23T12:00:00Z")));
    assert_eq!(item.lifecycle(), WorkItemLifecycle::Open);
    assert_eq!(item.revision(), Revision::initial());
}

#[test]
fn scenario_03_scheduled_reminder_is_hidden_and_fired_child_is_visible() {
    let fire_id = ReminderFireId::new();
    let mut reminder = Reminder::new(
        ReminderId::new(),
        ReminderTarget::WorkItem(WorkItemId::new()),
        at("2026-07-23T12:00:00Z"),
        fire_id,
    );
    assert!(!reminder.fires()[0].is_in_default_inbox());
    reminder.mark_fired(fire_id).expect("fire reminder");
    assert!(reminder.fires()[0].is_in_default_inbox());
}

#[test]
fn scenario_04_resolved_unread_signal_is_valid_and_visible() {
    let (occurrence_key, entity_key) = source_keys("completion-1");
    let receipt_id = SourceReceiptId::new();
    let entity_id = SourceEntityId::new();
    SourceReceipt::reconstruct(
        receipt_id,
        occurrence_key,
        Some(entity_key.clone()),
        at("2026-07-23T11:00:00Z"),
        at("2026-07-23T12:00:00Z"),
    )
    .expect("receipt permits independent clocks");
    SourceEntity::reconstruct(entity_id, entity_key).expect("source entity");
    let signal = AttentionSignal::reconstruct(
        AttentionSignalId::new(),
        Revision::initial(),
        SignalSourceLifecycle::Resolved,
        SignalAttentionState::Unread,
        receipt_id,
        Some(entity_id),
    )
    .expect("resolved unread signal");
    assert!(signal.is_in_default_inbox());
    assert_ne!(receipt_id.to_string(), entity_id.to_string());
}

#[test]
fn scenario_05_occurrence_key_is_distinct_and_value_comparable() {
    let (first, _) = source_keys("occurrence-1");
    let (same, _) = source_keys("occurrence-1");
    let (different, _) = source_keys("occurrence-2");
    assert_eq!(first, same);
    assert_ne!(first, different);
}

#[test]
fn scenario_06_new_receipt_can_share_entity_and_signal_identity() {
    let (_, entity_key) = source_keys("first");
    let entity_id = SourceEntityId::new();
    let signal_id = AttentionSignalId::new();
    let first_receipt = SourceReceiptId::new();
    let second_receipt = SourceReceiptId::new();
    let first = AttentionSignal::reconstruct(
        signal_id,
        Revision::initial(),
        SignalSourceLifecycle::Active,
        SignalAttentionState::Unread,
        first_receipt,
        Some(entity_id),
    )
    .expect("first signal view");
    let second = AttentionSignal::reconstruct(
        signal_id,
        Revision::try_from(2).expect("revision two"),
        SignalSourceLifecycle::Active,
        SignalAttentionState::Unread,
        second_receipt,
        Some(entity_id),
    )
    .expect("updated signal view");
    SourceEntity::reconstruct(entity_id, entity_key).expect("shared entity");
    assert_eq!(first.id(), second.id());
    assert_eq!(first.source_entity_id(), second.source_entity_id());
    assert_ne!(first.source_receipt_id(), second.source_receipt_id());
}

#[test]
fn scenario_07_active_acknowledged_signal_is_valid_and_hidden() {
    let signal = AttentionSignal::reconstruct(
        AttentionSignalId::new(),
        Revision::initial(),
        SignalSourceLifecycle::Active,
        SignalAttentionState::Acknowledged,
        SourceReceiptId::new(),
        None,
    )
    .expect("active acknowledged signal");
    assert!(!signal.is_in_default_inbox());
}

#[test]
fn scenario_08_resolved_acknowledged_signal_is_valid_and_hidden() {
    let mut signal = AttentionSignal::new(AttentionSignalId::new(), SourceReceiptId::new(), None);
    signal.acknowledge().expect("acknowledge signal");
    signal.resolve().expect("resolve acknowledged signal");
    assert_eq!(signal.attention_state(), SignalAttentionState::Acknowledged);
    assert_eq!(signal.source_lifecycle(), SignalSourceLifecycle::Resolved);
    assert!(!signal.is_in_default_inbox());
}

#[test]
fn scenario_09_snooze_consumes_fire_and_distinct_refire_reappears() {
    let original_id = ReminderFireId::new();
    let refire_id = ReminderFireId::new();
    let original_trigger = at("2026-07-23T12:00:00Z");
    let mut reminder = Reminder::new(
        ReminderId::new(),
        ReminderTarget::AttentionSignal(AttentionSignalId::new()),
        original_trigger,
        original_id,
    );
    reminder.mark_fired(original_id).expect("fire original");
    reminder
        .snooze_fire(original_id, refire_id, at("2026-07-23T13:00:00Z"))
        .expect("snooze original");
    assert_eq!(reminder.fires()[0].state(), ReminderFireState::Snoozed);
    assert!(!reminder.fires()[0].is_in_default_inbox());
    assert_eq!(reminder.trigger_at(), &original_trigger);
    reminder.mark_fired(refire_id).expect("fire replacement");
    assert!(reminder.fires()[1].is_in_default_inbox());
}

#[test]
fn scenario_10_revision_starts_at_one_and_checked_increment_is_monotonic() {
    let first = Revision::initial();
    let second = first.checked_increment().expect("revision two");
    let third = second.checked_increment().expect("revision three");
    assert_eq!([first.value(), second.value(), third.value()], [1, 2, 3]);
}
