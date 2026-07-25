mod fixture_helpers;

use attention_protocol::AttentionHelloResult;
use attention_protocol::ChangesResult;
use attention_protocol::CursorGap;
use attention_protocol::SnapshotResult;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;

#[test]
fn point_and_hello_snapshots_keep_cursor_outside_state() {
    assert_structural_round_trip::<SnapshotResult>("sync/snapshot.json");
    assert_structural_round_trip::<AttentionHelloResult>("sync/hello_snapshot.json");
}

#[test]
fn empty_multi_page_and_all_gap_reasons_round_trip() {
    for fixture in [
        "sync/changes_empty_page.json",
        "sync/changes_multi_page.json",
        "sync/gap_invalid.json",
        "sync/gap_future.json",
        "sync/gap_expired.json",
    ] {
        assert_structural_round_trip::<ChangesResult>(fixture);
    }
}

#[test]
fn cursor_gap_requires_literal_true() {
    assert_raw_invalid::<CursorGap>("sync/invalid_snapshot_not_required.json");
}
