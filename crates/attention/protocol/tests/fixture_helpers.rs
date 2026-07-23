use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub fn fixture_text(relative_path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/v1")
        .join(relative_path);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    text.strip_suffix('\n').unwrap_or(&text).to_string()
}

pub fn assert_structural_round_trip<T>(relative_path: &str)
where
    T: DeserializeOwned + Serialize,
{
    let text = fixture_text(relative_path);
    let expected: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => panic!("invalid fixture JSON in {relative_path}: {error}"),
    };
    let typed: T = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => panic!("fixture failed to deserialize in {relative_path}: {error}"),
    };
    let actual = match serde_json::to_value(typed) {
        Ok(value) => value,
        Err(error) => panic!("fixture failed to reserialize in {relative_path}: {error}"),
    };
    assert_eq!(actual, expected, "structural mismatch for {relative_path}");
}

pub fn assert_raw_invalid<T>(relative_path: &str)
where
    T: DeserializeOwned,
{
    let text = fixture_text(relative_path);
    assert!(
        serde_json::from_str::<T>(&text).is_err(),
        "invalid fixture unexpectedly deserialized: {relative_path}"
    );
}
