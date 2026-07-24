use attention_kernel::AttentionSignal;
use attention_kernel::AttentionSignalId;
use attention_kernel::InboxEntry;
use attention_kernel::ReminderFire;
use attention_kernel::ReminderFireId;
use attention_kernel::ReminderFireState;
use attention_kernel::Revision;
use attention_kernel::SignalAttentionState;
use attention_kernel::SignalSourceLifecycle;
use attention_kernel::SourceReceiptId;
use attention_kernel::WorkItem;
use attention_kernel::WorkItemId;
use attention_kernel::WorkItemLifecycle;
use chrono::DateTime;
use chrono::Utc;

#[expect(clippy::expect_used, reason = "fixed timestamp test fixture")]
fn at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-07-23T12:00:00Z")
        .expect("valid timestamp")
        .with_timezone(&Utc)
}

#[test]
fn only_open_work_items_are_visible() {
    for lifecycle in [
        WorkItemLifecycle::Open,
        WorkItemLifecycle::Completed,
        WorkItemLifecycle::Cancelled,
    ] {
        let id = WorkItemId::new();
        let item =
            WorkItem::reconstruct(id, Revision::initial(), lifecycle, None, None, None, None)
                .expect("valid work item");
        assert_eq!(
            item.is_in_default_inbox(),
            lifecycle == WorkItemLifecycle::Open
        );
        assert_eq!(
            item.inbox_entry(),
            (lifecycle == WorkItemLifecycle::Open).then_some(InboxEntry::WorkItem(id))
        );
    }
}

#[test]
fn signal_visibility_depends_only_on_attention_state() {
    for source in [
        SignalSourceLifecycle::Active,
        SignalSourceLifecycle::Resolved,
        SignalSourceLifecycle::Expired,
    ] {
        for attention in [
            SignalAttentionState::Unread,
            SignalAttentionState::Acknowledged,
        ] {
            let id = AttentionSignalId::new();
            let signal = AttentionSignal::reconstruct(
                id,
                Revision::initial(),
                source,
                attention,
                SourceReceiptId::new(),
                None,
            )
            .expect("valid signal");
            let visible = attention == SignalAttentionState::Unread;
            assert_eq!(signal.is_in_default_inbox(), visible);
            assert_eq!(
                signal.inbox_entry(),
                visible.then_some(InboxEntry::AttentionSignal(id))
            );
        }
    }
}

#[test]
fn only_fired_reminder_children_are_visible() {
    for state in [
        ReminderFireState::Scheduled,
        ReminderFireState::Fired,
        ReminderFireState::Acknowledged,
        ReminderFireState::Snoozed,
    ] {
        let id = ReminderFireId::new();
        let fire = ReminderFire::reconstruct(id, at(), state).expect("valid fire");
        let visible = state == ReminderFireState::Fired;
        assert_eq!(fire.is_in_default_inbox(), visible);
        assert_eq!(
            fire.inbox_entry(),
            visible.then_some(InboxEntry::ReminderFire(id))
        );
    }
}
