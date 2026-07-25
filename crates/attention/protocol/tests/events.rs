mod fixture_helpers;

use attention_protocol::ChangeEvent;
use attention_protocol::V1_CHANGE_KIND_NAMES;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use std::collections::HashSet;

#[test]
fn all_nine_event_kinds_have_complete_fixture_views() {
    assert_structural_round_trip::<Vec<ChangeEvent>>("events/all_kinds.json");
    let events: Vec<ChangeEvent> =
        serde_json::from_str(&fixture_helpers::fixture_text("events/all_kinds.json"))
            .expect("event fixtures");
    let fixture_kinds = events
        .iter()
        .map(|event| {
            serde_json::to_value(event.kind)
                .expect("serialize event kind")
                .as_str()
                .expect("serialized event kind string")
                .to_owned()
        })
        .collect::<HashSet<_>>();
    let catalog_kinds = V1_CHANGE_KIND_NAMES
        .iter()
        .map(|kind| (*kind).to_owned())
        .collect::<HashSet<_>>();
    assert_eq!(fixture_kinds, catalog_kinds);
    assert_raw_invalid::<ChangeEvent>("events/invalid_unknown_kind.json");
    assert_structural_round_trip::<ChangeEvent>("events/source_receipt_only.json");
}
