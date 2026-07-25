mod fixture_helpers;

use attention_protocol::CREATE_CONFLICT;
use attention_protocol::CURSOR_GAP;
use attention_protocol::DELIVERY_NOT_FOUND;
use attention_protocol::EXPECTED_REVISION_CONFLICT;
use attention_protocol::ErrorCode;
use attention_protocol::HELLO_REQUIRED;
use attention_protocol::IDEMPOTENCY_MISMATCH;
use attention_protocol::INTERNAL_ERROR;
use attention_protocol::INVALID_PARAMS;
use attention_protocol::INVALID_REQUEST;
use attention_protocol::METHOD_NOT_FOUND;
use attention_protocol::OCCURRENCE_CONTENT_MISMATCH;
use attention_protocol::PARSE_ERROR;
use attention_protocol::PROTOCOL_V1;
use attention_protocol::RESOURCE_NOT_FOUND;
use attention_protocol::ResourceRef;
use attention_protocol::RpcError;
use attention_protocol::SOURCE_VERSION_CONFLICT;
use attention_protocol::UNSUPPORTED_PROTOCOL_VERSION;
use attention_protocol::V1_ERROR_CODES;
use attention_protocol::V1Error;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use fixture_helpers::fixture_text;
use serde_json::Value;
use std::collections::HashSet;

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
    assert_eq!(codes["resource_not_found"], RESOURCE_NOT_FOUND.0);
    assert_eq!(
        codes["expected_revision_conflict"],
        EXPECTED_REVISION_CONFLICT.0
    );
    assert_eq!(codes["create_conflict"], CREATE_CONFLICT.0);
    assert_eq!(codes["idempotency_mismatch"], IDEMPOTENCY_MISMATCH.0);
    assert_eq!(
        codes["occurrence_content_mismatch"],
        OCCURRENCE_CONTENT_MISMATCH.0
    );
    assert_eq!(codes["source_version_conflict"], SOURCE_VERSION_CONFLICT.0);
    assert_eq!(codes["cursor_gap"], CURSOR_GAP.0);
    assert_eq!(codes["delivery_not_found"], DELIVERY_NOT_FOUND.0);
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

#[test]
fn all_known_v1_errors_round_trip_through_typed_conversion() {
    assert_structural_round_trip::<Vec<ResourceRef>>("errors/resource_refs.json");
    let errors: Vec<RpcError> =
        serde_json::from_str(&fixture_text("errors/v1_known.json")).expect("known errors fixture");
    let fixture_codes = errors
        .iter()
        .map(|error| error.code)
        .collect::<HashSet<_>>();
    let catalog_codes = V1_ERROR_CODES.iter().copied().collect::<HashSet<_>>();
    assert_eq!(fixture_codes, catalog_codes);
    for error in errors {
        let typed = V1Error::try_from(error.clone()).expect("typed known error");
        let round_trip = RpcError::try_from(typed).expect("serialize typed error");
        assert_eq!(round_trip, error);
    }
}

#[test]
fn malformed_known_data_is_rejected_but_unknown_codes_are_preserved() {
    let malformed: RpcError = serde_json::from_str(&fixture_text("errors/invalid_known_data.json"))
        .expect("open malformed error");
    assert!(V1Error::try_from(malformed).is_err());

    let unknown: RpcError =
        serde_json::from_str(&fixture_text("errors/unknown_code.json")).expect("unknown error");
    assert!(matches!(
        V1Error::try_from(unknown),
        Ok(V1Error::Unknown(_))
    ));
}
