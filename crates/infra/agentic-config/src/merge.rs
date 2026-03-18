//! TOML deep-merge implementation.
//!
//! Provides deep merge semantics for TOML Values:
//! - Tables merge recursively
//! - Arrays and scalars replace (no concatenation)
//! - No null-deletion semantics (TOML has no null type)

use toml::Value;

/// Deep-merge two TOML values.
///
/// # Semantics
/// - Table + Table: merge recursively, patch keys override base keys
/// - All other cases: patch replaces base entirely
///
/// # Examples
/// ```
/// use agentic_config::merge::deep_merge;
///
/// let base: toml::Value = toml::from_str(r#"
/// [section]
/// a = 1
/// b = 2
/// "#).unwrap();
///
/// let patch: toml::Value = toml::from_str(r#"
/// [section]
/// b = 3
/// c = 4
/// "#).unwrap();
///
/// let result = deep_merge(base, patch);
/// let section = result.get("section").unwrap().as_table().unwrap();
/// assert_eq!(section.get("a").unwrap().as_integer(), Some(1));
/// assert_eq!(section.get("b").unwrap().as_integer(), Some(3));
/// assert_eq!(section.get("c").unwrap().as_integer(), Some(4));
/// ```
pub fn deep_merge(base: Value, patch: Value) -> Value {
    match (base, patch) {
        (Value::Table(mut base_tbl), Value::Table(patch_tbl)) => {
            for (key, patch_val) in patch_tbl {
                match base_tbl.remove(&key) {
                    Some(Value::Table(base_sub)) if patch_val.is_table() => {
                        // Both are tables: recurse
                        let merged = deep_merge(Value::Table(base_sub), patch_val);
                        base_tbl.insert(key, merged);
                    }
                    _ => {
                        // Otherwise: patch wins
                        base_tbl.insert(key, patch_val);
                    }
                }
            }
            Value::Table(base_tbl)
        }
        // Non-table patch replaces base entirely
        (_, patch) => patch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn toml_from_str(s: &str) -> Value {
        toml::from_str(s).unwrap()
    }

    #[test]
    fn test_merge_disjoint_tables() {
        let base = toml_from_str("a = 1");
        let patch = toml_from_str("b = 2");
        let result = deep_merge(base, patch);
        assert_eq!(result.get("a").unwrap().as_integer(), Some(1));
        assert_eq!(result.get("b").unwrap().as_integer(), Some(2));
    }

    #[test]
    fn test_merge_overlapping_tables() {
        let base = toml_from_str("a = 1\nb = 2");
        let patch = toml_from_str("b = 3\nc = 4");
        let result = deep_merge(base, patch);
        assert_eq!(result.get("a").unwrap().as_integer(), Some(1));
        assert_eq!(result.get("b").unwrap().as_integer(), Some(3));
        assert_eq!(result.get("c").unwrap().as_integer(), Some(4));
    }

    #[test]
    fn test_merge_nested_tables() {
        let base = toml_from_str(
            r#"
            [section]
            x = 1
            y = 2
            "#,
        );
        let patch = toml_from_str(
            r#"
            [section]
            y = 3
            z = 4
            "#,
        );
        let result = deep_merge(base, patch);
        let section = result.get("section").unwrap().as_table().unwrap();
        assert_eq!(section.get("x").unwrap().as_integer(), Some(1));
        assert_eq!(section.get("y").unwrap().as_integer(), Some(3));
        assert_eq!(section.get("z").unwrap().as_integer(), Some(4));
    }

    #[test]
    fn test_arrays_replace() {
        let base = toml_from_str("a = [1, 2, 3]");
        let patch = toml_from_str("a = [4, 5]");
        let result = deep_merge(base, patch);
        let arr = result.get("a").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_integer(), Some(4));
        assert_eq!(arr[1].as_integer(), Some(5));
    }

    #[test]
    fn test_scalar_replaces_table() {
        let base = toml_from_str(
            r#"
            [section]
            nested = true
            "#,
        );
        let patch = toml_from_str("section = 42");
        let result = deep_merge(base, patch);
        assert_eq!(result.get("section").unwrap().as_integer(), Some(42));
    }

    #[test]
    fn test_table_replaces_scalar() {
        let base = toml_from_str("section = 42");
        let patch = toml_from_str(
            r#"
            [section]
            nested = true
            "#,
        );
        let result = deep_merge(base, patch);
        let section = result.get("section").unwrap().as_table().unwrap();
        assert_eq!(section.get("nested").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn test_empty_patch_is_identity() {
        let base = toml_from_str(
            r#"
            a = 1
            [section]
            b = 2
            "#,
        );
        let patch = toml_from_str("");
        let result = deep_merge(base.clone(), patch);
        assert_eq!(result, base);
    }

    #[test]
    fn test_empty_base_receives_patch() {
        let base = toml_from_str("");
        let patch = toml_from_str("a = 1");
        let result = deep_merge(base, patch);
        assert_eq!(result.get("a").unwrap().as_integer(), Some(1));
    }

    #[test]
    fn test_deeply_nested_merge() {
        let base = toml_from_str(
            r#"
            [level1.level2.level3]
            a = 1
            b = 2
            "#,
        );
        let patch = toml_from_str(
            r#"
            [level1.level2.level3]
            b = 99
            c = 3
            "#,
        );
        let result = deep_merge(base, patch);
        let level3 = result
            .get("level1")
            .unwrap()
            .get("level2")
            .unwrap()
            .get("level3")
            .unwrap()
            .as_table()
            .unwrap();
        assert_eq!(level3.get("a").unwrap().as_integer(), Some(1));
        assert_eq!(level3.get("b").unwrap().as_integer(), Some(99));
        assert_eq!(level3.get("c").unwrap().as_integer(), Some(3));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Property-based tests using proptest
    // ─────────────────────────────────────────────────────────────────────────
    //
    // Proptest generates random inputs to verify algebraic properties of deep_merge().
    // This catches edge cases that unit tests might miss (e.g., deeply nested structures,
    // unusual key combinations).
    //
    // The `proptest-regressions/merge.txt` file stores seeds for inputs that previously
    // caused failures. Proptest replays these seeds before generating new random inputs,
    // ensuring regressions are caught immediately.
    //
    // Two properties are tested:
    // 1. Identity: deep_merge(base, {}) == base (empty patch is no-op)
    // 2. Idempotence: deep_merge(deep_merge(base, patch), patch) == deep_merge(base, patch)
    //    (re-applying the same patch doesn't change the result)
    //
    // These properties validate that our merge semantics are sound for config overlays.
    proptest! {
        /// Identity property: merging with empty table returns original
        #[test]
        fn prop_empty_patch_is_identity(base in arb_toml_table()) {
            let empty = Value::Table(Default::default());
            let result = deep_merge(base.clone(), empty);
            prop_assert_eq!(result, base);
        }

        /// Idempotence property: applying same patch twice equals applying once
        #[test]
        fn prop_idempotent_merge(base in arb_toml_table(), patch in arb_toml_table()) {
            let once = deep_merge(base.clone(), patch.clone());
            let twice = deep_merge(once.clone(), patch);
            prop_assert_eq!(once, twice);
        }
    }

    // Helper to generate arbitrary TOML tables for property tests
    fn arb_toml_table() -> impl Strategy<Value = Value> {
        prop::collection::hash_map("[a-z]{1,3}", arb_toml_value(), 0..5)
            .prop_map(|m| Value::Table(m.into_iter().collect()))
    }

    fn arb_toml_value() -> impl Strategy<Value = Value> {
        prop_oneof![
            any::<bool>().prop_map(Value::Boolean),
            any::<i64>().prop_map(Value::Integer),
            "[a-z]{0,10}".prop_map(Value::String),
            // Nested table (limited depth)
            prop::collection::hash_map("[a-z]{1,2}", arb_leaf_value(), 0..3)
                .prop_map(|m| Value::Table(m.into_iter().collect())),
        ]
    }

    fn arb_leaf_value() -> impl Strategy<Value = Value> {
        prop_oneof![
            any::<bool>().prop_map(Value::Boolean),
            any::<i64>().prop_map(Value::Integer),
            "[a-z]{0,10}".prop_map(Value::String),
        ]
    }
}
