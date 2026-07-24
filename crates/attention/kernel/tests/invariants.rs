use attention_kernel::AttentionSignal;
use attention_kernel::AttentionSignalId;
use attention_kernel::InvariantError;
use attention_kernel::Reminder;
use attention_kernel::ReminderFire;
use attention_kernel::ReminderFireId;
use attention_kernel::ReminderFireState;
use attention_kernel::ReminderId;
use attention_kernel::ReminderTarget;
use attention_kernel::Revision;
use attention_kernel::SignalAttentionState;
use attention_kernel::SignalSourceLifecycle;
use attention_kernel::SourceReceiptId;
use attention_kernel::WorkItem;
use attention_kernel::WorkItemId;
use attention_kernel::WorkItemLifecycle;
use attention_kernel::validate_unique_reminder_targets;
use chrono::DateTime;
use chrono::Utc;

#[expect(clippy::expect_used, reason = "fixed timestamp test fixture")]
fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("valid timestamp")
        .with_timezone(&Utc)
}

#[expect(clippy::expect_used, reason = "validated reminder fire test fixture")]
fn fire(id: ReminderFireId, state: ReminderFireState) -> ReminderFire {
    ReminderFire::reconstruct(id, at("2026-07-23T12:00:00Z"), state).expect("valid fire")
}

#[test]
fn revision_rejects_zero_and_overflow() {
    assert_eq!(Revision::initial().value(), 1);
    assert!(matches!(
        Revision::try_from(0),
        Err(InvariantError::RevisionZero)
    ));
    assert!(matches!(
        Revision::try_from(u64::MAX)
            .expect("nonzero revision")
            .checked_increment(),
        Err(InvariantError::RevisionOverflow)
    ));
}

#[test]
fn work_item_transitions_once_and_rejects_terminal_changes() {
    let mut completed = WorkItem::new(WorkItemId::new(), None, None, None, None);
    completed.complete().expect("complete open item");
    assert_eq!(completed.lifecycle(), WorkItemLifecycle::Completed);
    assert_eq!(completed.revision().value(), 2);
    assert!(completed.complete().is_err());
    assert_eq!(completed.revision().value(), 2);

    let mut cancelled = WorkItem::new(WorkItemId::new(), None, None, None, None);
    cancelled.cancel().expect("cancel open item");
    assert_eq!(cancelled.lifecycle(), WorkItemLifecycle::Cancelled);
}

#[test]
fn all_six_signal_combinations_are_reconstructable() {
    for source in [
        SignalSourceLifecycle::Active,
        SignalSourceLifecycle::Resolved,
        SignalSourceLifecycle::Expired,
    ] {
        for attention in [
            SignalAttentionState::Unread,
            SignalAttentionState::Acknowledged,
        ] {
            let signal = AttentionSignal::reconstruct(
                AttentionSignalId::new(),
                Revision::initial(),
                source,
                attention,
                SourceReceiptId::new(),
                None,
            )
            .expect("valid independent signal dimensions");
            assert_eq!(signal.source_lifecycle(), source);
            assert_eq!(signal.attention_state(), attention);
        }
    }
}

#[test]
fn signal_transitions_are_independent_and_increment_once() {
    let mut signal = AttentionSignal::new(AttentionSignalId::new(), SourceReceiptId::new(), None);
    signal.acknowledge().expect("acknowledge unread signal");
    assert_eq!(signal.source_lifecycle(), SignalSourceLifecycle::Active);
    assert_eq!(signal.revision().value(), 2);
    signal.resolve().expect("resolve active signal");
    assert_eq!(signal.attention_state(), SignalAttentionState::Acknowledged);
    assert_eq!(signal.revision().value(), 3);
    assert!(signal.expire().is_err());
    assert_eq!(signal.revision().value(), 3);
}

#[test]
fn reminder_reconstruction_rejects_invalid_child_collections() {
    let target = ReminderTarget::WorkItem(WorkItemId::new());
    assert!(matches!(
        Reminder::reconstruct(
            ReminderId::new(),
            Revision::initial(),
            target,
            at("2026-07-23T12:00:00Z"),
            Vec::new(),
        ),
        Err(InvariantError::MissingReminderFire)
    ));

    let duplicate_id = ReminderFireId::new();
    assert!(matches!(
        Reminder::reconstruct(
            ReminderId::new(),
            Revision::initial(),
            target,
            at("2026-07-23T12:00:00Z"),
            vec![
                fire(duplicate_id, ReminderFireState::Acknowledged),
                fire(duplicate_id, ReminderFireState::Snoozed),
            ],
        ),
        Err(InvariantError::DuplicateReminderFireId(_))
    ));

    assert!(matches!(
        Reminder::reconstruct(
            ReminderId::new(),
            Revision::initial(),
            target,
            at("2026-07-23T12:00:00Z"),
            vec![
                fire(ReminderFireId::new(), ReminderFireState::Scheduled),
                fire(ReminderFireId::new(), ReminderFireState::Fired),
            ],
        ),
        Err(InvariantError::MultipleCurrentReminderFires)
    ));
}

#[test]
fn reminder_acknowledgement_and_snooze_preserve_retained_children() {
    let first_id = ReminderFireId::new();
    let trigger = at("2026-07-23T12:00:00Z");
    let mut acknowledged = Reminder::new(
        ReminderId::new(),
        ReminderTarget::WorkItem(WorkItemId::new()),
        trigger,
        first_id,
    );
    acknowledged
        .mark_fired(first_id)
        .expect("fire scheduled child");
    acknowledged
        .acknowledge_fire(first_id)
        .expect("acknowledge fired child");
    assert_eq!(
        acknowledged.fires()[0].state(),
        ReminderFireState::Acknowledged
    );
    assert_eq!(acknowledged.revision().value(), 3);
    assert!(acknowledged.acknowledge_fire(first_id).is_err());

    let original_id = ReminderFireId::new();
    let refire_id = ReminderFireId::new();
    let mut snoozed = Reminder::new(
        ReminderId::new(),
        ReminderTarget::AttentionSignal(AttentionSignalId::new()),
        trigger,
        original_id,
    );
    snoozed
        .mark_fired(original_id)
        .expect("fire original child");
    snoozed
        .snooze_fire(original_id, refire_id, at("2026-07-23T13:00:00Z"))
        .expect("snooze fired child");
    assert_eq!(snoozed.fires().len(), 2);
    assert_eq!(snoozed.fires()[0].state(), ReminderFireState::Snoozed);
    assert_eq!(snoozed.fires()[1].state(), ReminderFireState::Scheduled);
    assert_eq!(snoozed.trigger_at(), &trigger);
    assert!(matches!(
        snoozed.snooze_fire(refire_id, original_id, at("2026-07-23T14:00:00Z")),
        Err(InvariantError::SnoozeIdReuse(_))
    ));
}

#[test]
fn duplicate_reminder_targets_are_rejected() {
    let target = ReminderTarget::WorkItem(WorkItemId::new());
    let reminders = [
        Reminder::new(
            ReminderId::new(),
            target,
            at("2026-07-23T12:00:00Z"),
            ReminderFireId::new(),
        ),
        Reminder::new(
            ReminderId::new(),
            target,
            at("2026-07-23T13:00:00Z"),
            ReminderFireId::new(),
        ),
    ];
    assert!(matches!(
        validate_unique_reminder_targets(&reminders),
        Err(InvariantError::DuplicateReminderTarget)
    ));
}
