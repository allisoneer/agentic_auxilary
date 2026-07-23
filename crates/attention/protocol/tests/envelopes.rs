mod fixture_helpers;

use attention_protocol::RpcNotification;
use attention_protocol::RpcRequest;
use attention_protocol::RpcResponse;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use fixture_helpers::fixture_text;
use serde_json::Value;

#[test]
fn valid_request_and_notification_fixtures_round_trip_structurally() {
    for fixture in [
        "envelopes/request_with_params.json",
        "envelopes/request_without_params.json",
    ] {
        assert_structural_round_trip::<RpcRequest<Value>>(fixture);
    }
    assert_structural_round_trip::<RpcNotification<Value>>("envelopes/notification.json");
}

#[test]
fn valid_response_fixtures_round_trip_structurally() {
    for fixture in [
        "envelopes/response_success.json",
        "envelopes/response_correlated_error.json",
        "envelopes/response_parse_error_null_id.json",
        "envelopes/response_invalid_request_null_id.json",
    ] {
        assert_structural_round_trip::<RpcResponse<Value>>(fixture);
    }
}

#[test]
fn additive_unknown_fields_are_accepted_without_becoming_contract_state()
-> Result<(), serde_json::Error> {
    let request: RpcRequest<Value> =
        serde_json::from_str(&fixture_text("envelopes/request_additive_field.json"))?;
    let request = serde_json::to_value(request)?;
    assert!(request.get("future").is_none());
    for required in ["jsonrpc", "id", "method"] {
        assert!(request.get(required).is_some(), "missing {required}");
    }

    let response: RpcResponse<Value> =
        serde_json::from_str(&fixture_text("envelopes/response_additive_field.json"))?;
    let response = serde_json::to_value(response)?;
    assert!(response.get("future").is_none());
    for required in ["jsonrpc", "id", "result"] {
        assert!(response.get(required).is_some(), "missing {required}");
    }

    Ok(())
}

#[test]
fn malformed_requests_are_rejected_from_raw_text() {
    for fixture in [
        "envelopes/invalid_request_numeric_id.json",
        "envelopes/invalid_request_params_null.json",
        "envelopes/invalid_request_params_array.json",
        "envelopes/invalid_request_params_scalar.json",
        "envelopes/invalid_request_result_field.json",
        "envelopes/invalid_request_error_field.json",
        "envelopes/invalid_request_duplicate_id.json",
        "envelopes/invalid_missing_jsonrpc.json",
        "envelopes/invalid_wrong_jsonrpc.json",
        "envelopes/invalid_null_jsonrpc.json",
    ] {
        assert_raw_invalid::<RpcRequest<Value>>(fixture);
    }
}

#[test]
fn malformed_notifications_are_rejected_from_raw_text() {
    for fixture in [
        "envelopes/invalid_notification_id.json",
        "envelopes/invalid_notification_result_field.json",
        "envelopes/invalid_notification_error_field.json",
    ] {
        assert_raw_invalid::<RpcNotification<Value>>(fixture);
    }
}

#[test]
fn malformed_responses_are_rejected_from_raw_text() {
    for fixture in [
        "envelopes/invalid_response_both.json",
        "envelopes/invalid_response_neither.json",
        "envelopes/invalid_response_duplicate_result.json",
        "envelopes/invalid_response_missing_id.json",
        "envelopes/invalid_response_numeric_id.json",
        "envelopes/invalid_response_null_success_id.json",
        "envelopes/invalid_response_method_field.json",
        "envelopes/invalid_response_params_field.json",
        "envelopes/invalid_response_duplicate_jsonrpc.json",
    ] {
        assert_raw_invalid::<RpcResponse<Value>>(fixture);
    }
}
