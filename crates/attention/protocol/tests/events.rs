mod fixture_helpers;

use attention_protocol::ChangeEvent;
use attention_protocol::V1_CHANGE_KIND_NAMES;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;

#[test]
fn all_nine_event_kinds_have_complete_fixture_views() {
    assert_structural_round_trip::<Vec<ChangeEvent>>("events/all_kinds.json");
    let events: Vec<ChangeEvent> =
        serde_json::from_str(&fixture_helpers::fixture_text("events/all_kinds.json"))
            .expect("event fixtures");
    assert_eq!(events.len(), V1_CHANGE_KIND_NAMES.len());
    assert_raw_invalid::<ChangeEvent>("events/invalid_unknown_kind.json");
    assert_structural_round_trip::<ChangeEvent>("events/source_receipt_only.json");
}
