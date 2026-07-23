mod fixture_helpers;

use attention_protocol::ErrorCode;
use attention_protocol::HELLO_REQUIRED;
use attention_protocol::INTERNAL_ERROR;
use attention_protocol::INVALID_PARAMS;
use attention_protocol::INVALID_REQUEST;
use attention_protocol::METHOD_NOT_FOUND;
use attention_protocol::PARSE_ERROR;
use attention_protocol::PROTOCOL_V1;
use attention_protocol::RpcError;
use attention_protocol::UNSUPPORTED_PROTOCOL_VERSION;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use fixture_helpers::fixture_text;
use serde_json::Value;

#[test]
fn all_named_error_codes_match_the_v1_fixture() {
    let codes: Value =
        serde_json::from_str(&fixture_text("errors/named_codes.json")).expect("named-code fixture");
    assert_eq!(codes["parse_error"], PARSE_ERROR.0);
    assert_eq!(codes["invalid_request"], INVALID_REQUEST.0);
    assert_eq!(codes["method_not_found"], METHOD_NOT_FOUND.0);
    assert_eq!(codes["invalid_params"], INVALID_PARAMS.0);
    assert_eq!(codes["internal_error"], INTERNAL_ERROR.0);
    assert_eq!(
        codes["unsupported_protocol_version"],
        UNSUPPORTED_PROTOCOL_VERSION.0
    );
    assert_eq!(codes["hello_required"], HELLO_REQUIRED.0);
}

#[test]
fn unknown_codes_and_optional_data_round_trip() {
    for fixture in [
        "errors/unknown_code.json",
        "errors/data_absent.json",
        "errors/data_present.json",
    ] {
        assert_structural_round_trip::<RpcError>(fixture);
    }
    let unknown: RpcError = serde_json::from_str(&fixture_text("errors/unknown_code.json"))
        .expect("unknown error code");
    assert_eq!(unknown.code, ErrorCode(-32123));
}

#[test]
fn explicit_unsupported_version_mapping_matches_fixture() {
    let expected: RpcError = serde_json::from_str(&fixture_text("errors/unsupported_version.json"))
        .expect("unsupported-version fixture");
    assert_eq!(
        RpcError::unsupported_protocol_version(&[PROTOCOL_V1]),
        expected
    );
}

#[test]
fn malformed_error_code_is_rejected_from_raw_text() {
    assert_raw_invalid::<RpcError>("errors/invalid_string_code.json");
}
