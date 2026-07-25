mod fixture_helpers;

use attention_protocol::AttentionSignalView;
use attention_protocol::InboxEntryView;
use attention_protocol::ReminderView;
use attention_protocol::SourceEntityView;
use attention_protocol::SourceReceiptView;
use attention_protocol::WorkItemView;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;

#[test]
fn every_work_item_lifecycle_and_optional_shape_round_trips() {
    assert_structural_round_trip::<Vec<WorkItemView>>("resources/work_items/all_lifecycles.json");
}

#[test]
fn signal_source_and_attention_states_round_trip_independently() {
    assert_structural_round_trip::<Vec<AttentionSignalView>>("resources/signals/all_states.json");
}

#[test]
fn reminders_retain_acknowledged_snoozed_and_replacement_fires() {
    assert_structural_round_trip::<Vec<ReminderView>>("resources/reminders/retained_fires.json");
}

#[test]
fn source_receipts_and_entities_cover_order_modes() {
    assert_structural_round_trip::<Vec<SourceReceiptView>>("resources/source/receipts.json");
    assert_structural_round_trip::<Vec<SourceEntityView>>("resources/source/entities.json");
}

#[test]
fn inbox_entries_are_complete_and_renderable_without_refetch() {
    assert_structural_round_trip::<Vec<InboxEntryView>>("resources/inbox/all_entries.json");
    assert_raw_invalid::<InboxEntryView>("resources/inbox/invalid_unknown_kind.json");
}
