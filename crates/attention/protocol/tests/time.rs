mod fixture_helpers;

use attention_protocol::WireTimestamp;
use chrono::DateTime;
use chrono::Utc;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use fixture_helpers::fixture_text;

#[test]
fn canonical_timestamp_fixture_round_trips_exactly() {
    assert_structural_round_trip::<WireTimestamp>("time/canonical.json");
    let text = fixture_text("time/canonical.json");
    let timestamp: WireTimestamp =
        serde_json::from_str(&text).expect("canonical timestamp fixture");
    assert_eq!(
        serde_json::to_string(&timestamp).expect("serialize timestamp"),
        text
    );
}

#[test]
fn every_noncanonical_timestamp_fixture_is_rejected() {
    for fixture in [
        "time/invalid_no_fraction.json",
        "time/invalid_milliseconds.json",
        "time/invalid_nanoseconds.json",
        "time/invalid_offset.json",
        "time/invalid_lowercase_z.json",
        "time/invalid_missing_z.json",
    ] {
        assert_raw_invalid::<WireTimestamp>(fixture);
    }
}

#[test]
fn constructor_rejects_sub_microsecond_fixture_input() {
    let text = fixture_text("time/submicro_constructor.json");
    let value: String = serde_json::from_str(&text).expect("sub-microsecond fixture string");
    let datetime = DateTime::parse_from_rfc3339(&value)
        .expect("valid RFC3339 instant")
        .with_timezone(&Utc);
    assert!(WireTimestamp::from_datetime(datetime).is_err());
}
