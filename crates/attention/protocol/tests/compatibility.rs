mod fixture_helpers;

use attention_protocol::ErrorCode;
use attention_protocol::RpcRequest;
use attention_protocol::V1_CHANGE_KIND_NAMES;
use attention_protocol::V1_ERROR_CODES;
use attention_protocol::V1_METHOD_NAMES;
use attention_protocol::V1_NOTIFICATION_METHOD_NAMES;
use attention_protocol::V1_RESOURCE_VARIANT_NAMES;
use attention_protocol::V1_WORKER_VARIANT_NAMES;
use attention_protocol::WorkItemView;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use fixture_helpers::fixture_text;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct CoverageManifest {
    methods: Vec<String>,
    notifications: Vec<String>,
    event_kinds: Vec<String>,
    error_codes: Vec<i32>,
    resource_variants: Vec<String>,
    worker_variants: Vec<String>,
    required_fixture_categories: Vec<String>,
}

#[test]
fn coverage_manifest_exactly_matches_every_governed_catalog() {
    let manifest: CoverageManifest =
        serde_json::from_str(&fixture_text("coverage_manifest.json")).expect("coverage manifest");
    assert_eq!(manifest.methods, V1_METHOD_NAMES);
    assert_eq!(manifest.notifications, V1_NOTIFICATION_METHOD_NAMES);
    assert_eq!(manifest.event_kinds, V1_CHANGE_KIND_NAMES);
    assert_eq!(
        manifest.error_codes,
        V1_ERROR_CODES
            .iter()
            .map(|ErrorCode(code)| *code)
            .collect::<Vec<_>>()
    );
    assert_eq!(manifest.resource_variants, V1_RESOURCE_VARIANT_NAMES);
    assert_eq!(manifest.worker_variants, V1_WORKER_VARIANT_NAMES);

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1");
    for category in manifest.required_fixture_categories {
        assert!(
            root.join(&category).is_dir(),
            "missing fixture category {category}"
        );
    }
}

#[test]
fn additive_duplicate_null_and_unknown_tag_compatibility_is_frozen() {
    let work_item: WorkItemView = serde_json::from_str(&fixture_text(
        "compatibility/resource_additive_and_null.json",
    ))
    .expect("additive resource");
    let serialized = serde_json::to_value(work_item).expect("serialize resource");
    assert!(serialized.get("future").is_none());
    assert!(serialized.get("due_at").is_none());

    assert_raw_invalid::<WorkItemView>("compatibility/resource_duplicate_required.json");
    assert_raw_invalid::<RpcRequest<Value>>("methods/invalid_array_params.json");
    assert_structural_round_trip::<Value>("time/canonical.json");
}

#[test]
fn protocol_manifest_has_no_forbidden_dependency() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("protocol manifest");
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .and_then(|tail| tail.split("[[test]]").next())
        .expect("dependencies section");
    for forbidden in [
        "attention-kernel",
        "base64",
        "sqlx",
        "tokio",
        "tauri",
        "discord",
    ] {
        assert!(
            !dependencies.contains(forbidden),
            "forbidden dependency {forbidden}"
        );
    }
}
