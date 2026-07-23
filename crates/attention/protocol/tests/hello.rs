mod fixture_helpers;

use attention_protocol::HelloRequest;
use attention_protocol::HelloResult;
use attention_protocol::RpcError;
use attention_protocol::SubscriptionRequest;
use attention_protocol::SubscriptionResult;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use fixture_helpers::fixture_text;
use serde_json::Value;

#[test]
fn hello_request_modes_round_trip_structurally() {
    for fixture in [
        "hello/request_none.json",
        "hello/request_snapshot.json",
        "hello/request_resume.json",
    ] {
        assert_structural_round_trip::<HelloRequest>(fixture);
    }
}

#[test]
fn hello_result_modes_round_trip_with_generic_empty_state() {
    for fixture in [
        "hello/result_none.json",
        "hello/result_snapshot.json",
        "hello/result_resume.json",
    ] {
        assert_structural_round_trip::<HelloResult<Value>>(fixture);
    }
}

#[test]
fn hello_wire_names_and_nested_fields_are_exact() {
    let request: HelloRequest = serde_json::from_str(&fixture_text("hello/request_resume.json"))
        .expect("resume request fixture");
    assert!(matches!(
        request.subscription,
        SubscriptionRequest::Resume { .. }
    ));

    let result: HelloResult<Value> =
        serde_json::from_str(&fixture_text("hello/result_snapshot.json"))
            .expect("snapshot result fixture");
    assert!(matches!(
        result.subscription_result,
        SubscriptionResult::Snapshot { .. }
    ));
    assert_eq!(result.limits.max_message_bytes, 1_048_576);
    assert_eq!(result.limits.max_in_flight, 32);
}

#[test]
fn unknown_modes_are_rejected_from_raw_text() {
    assert_raw_invalid::<HelloRequest>("hello/invalid_request_unknown_mode.json");
    assert_raw_invalid::<HelloResult<Value>>("hello/invalid_result_unknown_mode.json");
}

#[test]
fn duplicate_hello_fields_are_rejected_from_raw_text() {
    assert_raw_invalid::<HelloRequest>("hello/invalid_request_duplicate_protocol_version.json");
    assert_raw_invalid::<HelloRequest>("hello/invalid_request_duplicate_mode.json");
    assert_raw_invalid::<HelloResult<Value>>("hello/invalid_result_duplicate_limits.json");
}

#[test]
fn unsupported_version_error_fixture_is_sanitized() {
    assert_structural_round_trip::<RpcError>("hello/unsupported_version_error.json");
    let error: RpcError =
        serde_json::from_str(&fixture_text("hello/unsupported_version_error.json"))
            .expect("unsupported-version error fixture");
    assert_eq!(error.code.0, -32090);
    assert_eq!(
        error.data.expect("supported versions data")["supported_versions"],
        serde_json::json!([1])
    );
}
