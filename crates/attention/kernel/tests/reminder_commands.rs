use attention_kernel::AcknowledgeReminderFire;
use attention_kernel::ChangeEventId;
use attention_kernel::CreateReminder;
use attention_kernel::EvaluationContext;
use attention_kernel::FireReminder;
use attention_kernel::MutationIdempotencyKey;
use attention_kernel::OutboxIntentId;
use attention_kernel::ReminderFireId;
use attention_kernel::ReminderFireState;
use attention_kernel::ReminderId;
use attention_kernel::ReminderTarget;
use attention_kernel::SnoozeReminderFire;
use attention_kernel::WorkItemId;
use attention_kernel::evaluate_acknowledge_reminder_fire;
use attention_kernel::evaluate_create_reminder;
use attention_kernel::evaluate_fire_reminder;
use attention_kernel::evaluate_snooze_reminder_fire;
use chrono::DateTime;
use chrono::Utc;

#[expect(clippy::expect_used, reason = "fixed reminder fixtures")]
fn at(hour: u32) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&format!("2026-07-24T{hour:02}:00:00Z"))
        .expect("timestamp")
        .with_timezone(&Utc)
}

fn context(intent: bool) -> EvaluationContext {
    EvaluationContext::new(
        ChangeEventId::new(),
        intent.then(OutboxIntentId::new),
        at(12),
    )
}

#[test]
fn reminder_create_fire_acknowledge_and_snooze_are_complete_atomic_bundles() {
    let reminder_id = ReminderId::new();
    let fire_id = ReminderFireId::new();
    let create = CreateReminder::new(
        reminder_id,
        fire_id,
        ReminderTarget::WorkItem(WorkItemId::new()),
        at(12),
        MutationIdempotencyKey::new(),
    );
    let created = evaluate_create_reminder(&create, context(false));
    assert_eq!(
        created.root().fires()[0].state(),
        ReminderFireState::Scheduled
    );

    let fire = FireReminder::new(reminder_id, fire_id, MutationIdempotencyKey::new());
    let fired = evaluate_fire_reminder(&fire, created.root(), context(true))
        .expect("fire scheduled reminder");
    assert_eq!(fired.root().fires()[0].state(), ReminderFireState::Fired);
    assert!(fired.effects().outbox_intent().is_some());
    assert_eq!(
        fired.effects().change().inbox_effects().additions().len(),
        1
    );

    let acknowledge = AcknowledgeReminderFire::new(
        reminder_id,
        fire_id,
        fired.root().revision(),
        MutationIdempotencyKey::new(),
    );
    let acknowledged =
        evaluate_acknowledge_reminder_fire(&acknowledge, fired.root(), context(false))
            .expect("acknowledge fired reminder");
    assert_eq!(
        acknowledged.root().fires()[0].state(),
        ReminderFireState::Acknowledged
    );

    let second_reminder = evaluate_create_reminder(
        &CreateReminder::new(
            ReminderId::new(),
            ReminderFireId::new(),
            ReminderTarget::WorkItem(WorkItemId::new()),
            at(12),
            MutationIdempotencyKey::new(),
        ),
        context(false),
    );
    let second_fire_id = second_reminder.root().fires()[0].id();
    let second_fired = evaluate_fire_reminder(
        &FireReminder::new(
            second_reminder.root().id(),
            second_fire_id,
            MutationIdempotencyKey::new(),
        ),
        second_reminder.root(),
        context(false),
    )
    .expect("fire second reminder");
    let replacement = ReminderFireId::new();
    let snooze = SnoozeReminderFire::new(
        second_fired.root().id(),
        second_fire_id,
        replacement,
        at(13),
        second_fired.root().revision(),
        MutationIdempotencyKey::new(),
    );
    let snoozed = evaluate_snooze_reminder_fire(&snooze, second_fired.root(), context(false))
        .expect("snooze fired reminder");
    assert_eq!(snoozed.root().fires().len(), 2);
    assert_eq!(snoozed.root().fires()[1].id(), replacement);
}
