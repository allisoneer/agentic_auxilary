mod fixture_helpers;

use attention_protocol::BootId;
use attention_protocol::Cursor;
use attention_protocol::RequestId;
use attention_protocol::ResponseId;
use attention_protocol::ServerId;
use attention_protocol::StreamId;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use fixture_helpers::fixture_text;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct IdFixture {
    request_id: RequestId,
    server_id: ServerId,
    stream_id: StreamId,
    boot_id: BootId,
    cursor: Cursor,
    response_string: ResponseId,
    response_null: ResponseId,
}

#[test]
fn distinct_opaque_id_wrappers_round_trip_structurally() {
    assert_structural_round_trip::<IdFixture>("ids/distinct_ids.json");
    let ids: IdFixture = match serde_json::from_str(&fixture_text("ids/distinct_ids.json")) {
        Ok(value) => value,
        Err(error) => panic!("distinct IDs fixture: {error}"),
    };
    assert_eq!(ids.request_id, RequestId(String::new()));
    assert_eq!(ids.response_null, ResponseId::Null);
}

#[test]
fn non_string_request_and_numeric_response_ids_are_rejected() {
    assert_raw_invalid::<RequestId>("ids/invalid_request_numeric.json");
    assert_raw_invalid::<ResponseId>("ids/invalid_response_numeric.json");
}
