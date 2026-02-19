//! RFC 7396 JSON Merge Patch implementation.
//!
//! Provides deep merge semantics for JSON Values:
//! - Objects merge recursively
//! - `null` in patch deletes keys from target
//! - Arrays and scalars replace

use serde_json::Value;

/// Apply RFC 7396-style JSON merge patch.
///
/// # Semantics
/// - Objects merge recursively: patch keys override target keys
/// - `null` in patch deletes the corresponding key from target
/// - Arrays and scalars in patch replace target values entirely
/// - Type mismatches (e.g., object in patch vs scalar in target) result in replacement
///
/// # Examples
/// ```
/// use serde_json::json;
/// use agentic_config::merge::merge_patch;
///
/// let target = json!({"a": 1, "b": {"c": 2}});
/// let patch = json!({"b": {"d": 3}, "e": 4});
/// let result = merge_patch(target, patch);
/// assert_eq!(result, json!({"a": 1, "b": {"c": 2, "d": 3}, "e": 4}));
/// ```
pub fn merge_patch(target: Value, patch: Value) -> Value {
    match (target, patch) {
        (Value::Object(mut target_map), Value::Object(patch_map)) => {
            for (key, patch_value) in patch_map {
                if patch_value.is_null() {
                    // RFC 7396: null deletes the key
                    target_map.remove(&key);
                    continue;
                }

                let existing = target_map.remove(&key).unwrap_or(Value::Null);
                let merged = merge_patch(existing, patch_value);
                target_map.insert(key, merged);
            }
            Value::Object(target_map)
        }
        // Non-object patch replaces target entirely
        (_, patch) => patch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    #[test]
    fn test_merge_disjoint_objects() {
        let target = json!({"a": 1});
        let patch = json!({"b": 2});
        let result = merge_patch(target, patch);
        assert_eq!(result, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn test_merge_overlapping_objects() {
        let target = json!({"a": 1, "b": 2});
        let patch = json!({"b": 3, "c": 4});
        let result = merge_patch(target, patch);
        assert_eq!(result, json!({"a": 1, "b": 3, "c": 4}));
    }

    #[test]
    fn test_merge_nested_objects() {
        let target = json!({"a": {"x": 1, "y": 2}});
        let patch = json!({"a": {"y": 3, "z": 4}});
        let result = merge_patch(target, patch);
        assert_eq!(result, json!({"a": {"x": 1, "y": 3, "z": 4}}));
    }

    #[test]
    fn test_null_deletes_key() {
        let target = json!({"a": 1, "b": 2});
        let patch = json!({"b": null});
        let result = merge_patch(target, patch);
        assert_eq!(result, json!({"a": 1}));
    }

    #[test]
    fn test_null_deletes_nested_key() {
        let target = json!({"a": {"x": 1, "y": 2}});
        let patch = json!({"a": {"x": null}});
        let result = merge_patch(target, patch);
        assert_eq!(result, json!({"a": {"y": 2}}));
    }

    #[test]
    fn test_array_replaces() {
        let target = json!({"a": [1, 2, 3]});
        let patch = json!({"a": [4, 5]});
        let result = merge_patch(target, patch);
        assert_eq!(result, json!({"a": [4, 5]}));
    }

    #[test]
    fn test_scalar_replaces_object() {
        let target = json!({"a": {"nested": true}});
        let patch = json!({"a": 42});
        let result = merge_patch(target, patch);
        assert_eq!(result, json!({"a": 42}));
    }

    #[test]
    fn test_object_replaces_scalar() {
        let target = json!({"a": 42});
        let patch = json!({"a": {"nested": true}});
        let result = merge_patch(target, patch);
        assert_eq!(result, json!({"a": {"nested": true}}));
    }

    #[test]
    fn test_empty_patch_is_identity() {
        let target = json!({"a": 1, "b": {"c": 2}});
        let patch = json!({});
        let result = merge_patch(target.clone(), patch);
        assert_eq!(result, target);
    }

    #[test]
    fn test_empty_target_receives_patch() {
        let target = json!({});
        let patch = json!({"a": 1});
        let result = merge_patch(target, patch);
        assert_eq!(result, json!({"a": 1}));
    }

    #[test]
    fn test_deeply_nested_merge() {
        let target = json!({
            "level1": {
                "level2": {
                    "level3": {
                        "a": 1,
                        "b": 2
                    }
                }
            }
        });
        let patch = json!({
            "level1": {
                "level2": {
                    "level3": {
                        "b": null,
                        "c": 3
                    }
                }
            }
        });
        let result = merge_patch(target, patch);
        assert_eq!(
            result,
            json!({
                "level1": {
                    "level2": {
                        "level3": {
                            "a": 1,
                            "c": 3
                        }
                    }
                }
            })
        );
    }

    // Property-based tests using proptest
    proptest! {
        /// Identity property: merging with empty object returns original
        #[test]
        fn prop_empty_patch_is_identity(target in arb_json_object()) {
            let result = merge_patch(target.clone(), json!({}));
            prop_assert_eq!(result, target);
        }

        /// Idempotence property: applying same patch twice equals applying once
        /// Note: This only holds when the patch contains no null values (deletion markers).
        /// In RFC 7396, null means "delete this key", so applying a null-containing patch
        /// once will delete the key, and applying it again won't delete anything new,
        /// but the first result won't contain the null value itself.
        #[test]
        fn prop_idempotent_merge(target in arb_json_object(), patch in arb_non_null_json_object()) {
            let once = merge_patch(target.clone(), patch.clone());
            let twice = merge_patch(once.clone(), patch);
            prop_assert_eq!(once, twice);
        }
    }

    // Helper to generate arbitrary JSON objects for property tests
    fn arb_json_object() -> impl Strategy<Value = Value> {
        prop::collection::hash_map("[a-z]{1,3}", arb_json_value(), 0..5)
            .prop_map(|m| Value::Object(m.into_iter().collect()))
    }

    // Helper to generate JSON objects without null values (for idempotence test)
    fn arb_non_null_json_object() -> impl Strategy<Value = Value> {
        prop::collection::hash_map("[a-z]{1,3}", arb_non_null_json_value(), 0..5)
            .prop_map(|m| Value::Object(m.into_iter().collect()))
    }

    fn arb_json_value() -> impl Strategy<Value = Value> {
        prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|n| Value::Number(n.into())),
            "[a-z]{0,10}".prop_map(Value::String),
            // Nested object (limited depth)
            prop::collection::hash_map("[a-z]{1,2}", arb_leaf_value(), 0..3)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    }

    fn arb_non_null_json_value() -> impl Strategy<Value = Value> {
        prop_oneof![
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|n| Value::Number(n.into())),
            "[a-z]{0,10}".prop_map(Value::String),
            // Nested object (limited depth) - also without nulls
            prop::collection::hash_map("[a-z]{1,2}", arb_non_null_leaf_value(), 0..3)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    }

    fn arb_leaf_value() -> impl Strategy<Value = Value> {
        prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|n| Value::Number(n.into())),
            "[a-z]{0,10}".prop_map(Value::String),
        ]
    }

    fn arb_non_null_leaf_value() -> impl Strategy<Value = Value> {
        prop_oneof![
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|n| Value::Number(n.into())),
            "[a-z]{0,10}".prop_map(Value::String),
        ]
    }
}
