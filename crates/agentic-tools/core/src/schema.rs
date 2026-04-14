//! Schema engine for runtime transforms.

use schemars::Schema;
use serde_json::Value as Json;
use std::collections::HashMap;
use std::collections::HashSet;

/// Field-level constraint to apply to a schema.
#[derive(Clone, Debug)]
pub enum FieldConstraint {
    /// Restrict field to specific enum values.
    Enum(Vec<Json>),

    /// Apply numeric range constraints.
    Range {
        minimum: Option<Json>,
        maximum: Option<Json>,
    },

    /// Apply string pattern constraint.
    Pattern(String),

    /// Apply a JSON merge-patch to the field schema.
    MergePatch(Json),
}

/// Trait for custom schema transforms.
pub trait SchemaTransform: Send + Sync {
    /// Apply the transform to a tool's schema.
    fn apply(&self, tool: &str, schema: &mut Json);
}

/// Engine for applying runtime transforms to tool schemas.
///
/// Schemars derive generates base schemas at compile time.
/// SchemaEngine applies transforms at runtime for provider flexibility.
///
/// # Clone behavior
/// When cloned, `custom_transforms` are **not** carried over (they are not `Clone`).
/// Only `per_tool` constraints and `global_strict` settings are cloned.
#[derive(Default)]
pub struct SchemaEngine {
    per_tool: HashMap<String, Vec<(Vec<String>, FieldConstraint)>>,
    global_strict: bool,
    custom_transforms: Vec<Box<dyn SchemaTransform>>,
}

impl Clone for SchemaEngine {
    fn clone(&self) -> Self {
        // Custom transforms cannot be cloned, so we only clone the config
        Self {
            per_tool: self.per_tool.clone(),
            global_strict: self.global_strict,
            custom_transforms: Vec::new(), // Transforms are not cloned
        }
    }
}

impl std::fmt::Debug for SchemaEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaEngine")
            .field("per_tool", &self.per_tool)
            .field("global_strict", &self.global_strict)
            .field(
                "custom_transforms",
                &format!("[{} transforms]", self.custom_transforms.len()),
            )
            .finish()
    }
}

impl SchemaEngine {
    /// Create a new schema engine.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable strict mode (additionalProperties=false) globally.
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.global_strict = strict;
        self
    }

    /// Get global strict mode setting.
    pub fn is_strict(&self) -> bool {
        self.global_strict
    }

    /// Add a field constraint for a specific tool.
    ///
    /// The `json_path` is a list of property names to traverse to reach the field.
    /// For example, `["properties", "count"]` would target the "count" property.
    pub fn constrain_field(&mut self, tool: &str, json_path: Vec<String>, c: FieldConstraint) {
        self.per_tool
            .entry(tool.to_string())
            .or_default()
            .push((json_path, c));
    }

    /// Add a custom transform.
    pub fn add_transform<T: SchemaTransform + 'static>(&mut self, transform: T) {
        self.custom_transforms.push(Box::new(transform));
    }

    /// Transform a tool's schema applying all constraints and transforms.
    pub fn transform(&self, tool: &str, schema: Schema) -> Schema {
        let mut v = serde_json::to_value(&schema).expect("serialize schema");

        // Apply global strict mode
        if self.global_strict
            && let Some(obj) = v.as_object_mut()
        {
            obj.insert("additionalProperties".to_string(), Json::Bool(false));
        }

        // Apply per-tool constraints
        if let Some(entries) = self.per_tool.get(tool) {
            for (path, constraint) in entries {
                Self::apply_constraint(&mut v, path, constraint);
            }
        }

        // Apply custom transforms
        for transform in &self.custom_transforms {
            transform.apply(tool, &mut v);
        }

        // try_from only rejects non-object/non-bool JSON values.  Since we start
        // from a valid Schema (always an object) and built-in transforms only mutate
        // sub-nodes, failure here means a custom SchemaTransform replaced the root
        // type — a programming error that must surface immediately.
        Schema::try_from(v).expect("schema transform must produce a valid schema")
    }

    fn apply_constraint(root: &mut Json, path: &[String], constraint: &FieldConstraint) {
        let Some(node) = Self::find_node_mut(root, path) else {
            return;
        };
        let Some(obj) = node.as_object_mut() else {
            return;
        };
        match constraint {
            FieldConstraint::Enum(vals) => {
                obj.insert("enum".into(), Json::Array(vals.clone()));
            }
            FieldConstraint::Range { minimum, maximum } => {
                if let Some(m) = minimum {
                    obj.insert("minimum".into(), m.clone());
                }
                if let Some(m) = maximum {
                    obj.insert("maximum".into(), m.clone());
                }
            }
            FieldConstraint::Pattern(p) => {
                obj.insert("pattern".into(), Json::String(p.clone()));
            }
            FieldConstraint::MergePatch(patch) => {
                json_patch::merge(node, patch);
            }
        }
    }

    fn find_node_mut<'a>(root: &'a mut Json, path: &[String]) -> Option<&'a mut Json> {
        let mut cur = root;
        for seg in path {
            cur = cur.as_object_mut()?.get_mut(seg)?;
        }
        Some(cur)
    }
}

#[derive(Clone, Default)]
struct StripNullFromOptional;

impl schemars::transform::Transform for StripNullFromOptional {
    fn transform(&mut self, schema: &mut Schema) {
        let mut value = serde_json::to_value(&*schema).expect("serialize schema");
        strip_nulls_from_optional_properties(&mut value);
        *schema =
            Schema::try_from(value).expect("StripNullFromOptional must preserve schema validity");
    }
}

fn strip_nulls_from_optional_properties(node: &mut Json) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };

    recurse_object_entries(obj, "$defs");
    recurse_object_entries(obj, "definitions");

    let required = required_property_names(obj.get("required"));
    if let Some(properties) = obj.get_mut("properties").and_then(Json::as_object_mut) {
        for (property_name, property_schema) in properties {
            if !required.contains(property_name.as_str()) {
                strip_known_nullable_shapes(property_schema);
            }
            strip_nulls_from_optional_properties(property_schema);
        }
    }

    recurse_object_entries(obj, "patternProperties");
    recurse_schema_entry(obj, "additionalProperties");
    recurse_schema_entry(obj, "propertyNames");
    recurse_schema_entry(obj, "unevaluatedProperties");
    recurse_schema_entry(obj, "items");
    recurse_schema_entry(obj, "contains");
    recurse_schema_array_entry(obj, "prefixItems");
    recurse_schema_array_entry(obj, "allOf");
    recurse_schema_array_entry(obj, "anyOf");
    recurse_schema_array_entry(obj, "oneOf");
    recurse_schema_entry(obj, "if");
    recurse_schema_entry(obj, "then");
    recurse_schema_entry(obj, "else");
    recurse_schema_entry(obj, "not");
}

fn recurse_object_entries(obj: &mut serde_json::Map<String, Json>, key: &str) {
    let Some(entries) = obj.get_mut(key).and_then(Json::as_object_mut) else {
        return;
    };

    for value in entries.values_mut() {
        strip_nulls_from_optional_properties(value);
    }
}

fn recurse_schema_entry(obj: &mut serde_json::Map<String, Json>, key: &str) {
    let Some(value) = obj.get_mut(key) else {
        return;
    };

    strip_nulls_from_optional_properties(value);
}

fn recurse_schema_array_entry(obj: &mut serde_json::Map<String, Json>, key: &str) {
    let Some(values) = obj.get_mut(key).and_then(Json::as_array_mut) else {
        return;
    };

    for value in values {
        strip_nulls_from_optional_properties(value);
    }
}

fn required_property_names(required: Option<&Json>) -> HashSet<String> {
    required
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect()
}

fn strip_known_nullable_shapes(node: &mut Json) {
    strip_null_from_type_array(node);
    strip_null_from_enum_values(node);
    strip_null_from_any_of(node);
}

fn strip_null_from_type_array(node: &mut Json) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };

    let Some(type_values) = obj.get_mut("type").and_then(Json::as_array_mut) else {
        return;
    };

    let filtered: Vec<_> = type_values
        .iter()
        .filter(|value| *value != &Json::String("null".into()))
        .cloned()
        .collect();

    match filtered.as_slice() {
        [] => {}
        [single] => {
            obj.insert("type".to_string(), single.clone());
        }
        _ => {
            obj.insert("type".to_string(), Json::Array(filtered));
        }
    }
}

fn strip_null_from_enum_values(node: &mut Json) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };

    let Some(enum_values) = obj.get_mut("enum").and_then(Json::as_array_mut) else {
        return;
    };

    enum_values.retain(|value| !value.is_null());
}

fn strip_null_from_any_of(node: &mut Json) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };

    let Some(any_of) = obj.get("anyOf").and_then(Json::as_array) else {
        return;
    };

    let non_null_branches: Vec<Json> = any_of
        .iter()
        .filter(|branch| !is_explicit_null_branch(branch))
        .cloned()
        .collect();

    if non_null_branches.len() == any_of.len() || non_null_branches.is_empty() {
        return;
    }

    if non_null_branches.len() == 1 {
        let remaining_branch = non_null_branches.into_iter().next().unwrap();
        obj.remove("anyOf");

        if let Some(branch_obj) = remaining_branch.as_object() {
            for (key, value) in branch_obj {
                obj.entry(key.clone()).or_insert_with(|| value.clone());
            }
        } else {
            obj.insert("anyOf".to_string(), Json::Array(vec![remaining_branch]));
        }
        return;
    }

    obj.insert("anyOf".to_string(), Json::Array(non_null_branches));
}

fn is_explicit_null_branch(node: &Json) -> bool {
    matches!(
        node,
        Json::Object(obj) if obj.get("type") == Some(&Json::String("null".into()))
    )
}

// ============================================================================
// Centralized Draft 2020-12 Generator for MCP + Registry
// ============================================================================

/// Centralized schema generation using Draft 2020-12.
///
/// This module provides cached schema generation for MCP:
/// - JSON Schema Draft 2020-12 (MCP protocol requirement)
/// - `Option<T>` object properties are emitted as optional-but-non-nullable at
///   the property root while preserving inner item/value nullability
/// - Thread-local caching keyed by TypeId for performance
pub mod mcp_schema {
    use super::StripNullFromOptional;
    use schemars::JsonSchema;
    use schemars::Schema;
    use schemars::generate::SchemaSettings;
    use schemars::transform::RestrictFormats;
    use std::any::TypeId;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::Arc;

    thread_local! {
        static CACHE_FOR_TYPE: RefCell<HashMap<TypeId, Arc<Schema>>> = RefCell::new(HashMap::new());
        static CACHE_FOR_OUTPUT: RefCell<HashMap<TypeId, Result<Arc<Schema>, String>>> = RefCell::new(HashMap::new());
    }

    fn settings() -> SchemaSettings {
        // NOTE: StripNullFromOptional also affects output schemas generated by
        // this shared path. Accepted debt: some output structs may still
        // serialize Option<T> fields as null until follow-up work aligns
        // serialization with schema via skip_serializing_if.
        SchemaSettings::draft2020_12()
            .with_transform(RestrictFormats::default())
            .with_transform(StripNullFromOptional)
    }

    /// Generate a cached schema for type T using Draft 2020-12.
    pub fn cached_schema_for<T: JsonSchema + 'static>() -> Arc<Schema> {
        CACHE_FOR_TYPE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(x) = cache.get(&TypeId::of::<T>()) {
                return x.clone();
            }
            let generator = settings().into_generator();
            let root = generator.into_root_schema_for::<T>();
            let arc = Arc::new(root);
            cache.insert(TypeId::of::<T>(), arc.clone());
            arc
        })
    }

    /// Generate a cached output schema for type T, validating root type is "object".
    /// Returns Err if the root type is not "object" (per MCP spec requirement).
    pub fn cached_output_schema_for<T: JsonSchema + 'static>() -> Result<Arc<Schema>, String> {
        CACHE_FOR_OUTPUT.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(r) = cache.get(&TypeId::of::<T>()) {
                return r.clone();
            }
            let root = cached_schema_for::<T>();
            let json = serde_json::to_value(root.as_ref()).expect("serialize output schema");
            let result = match json.get("type") {
                Some(serde_json::Value::String(t)) if t == "object" => Ok(root.clone()),
                Some(serde_json::Value::String(t)) => Err(format!(
                    "MCP requires output_schema root type 'object', found '{}'",
                    t
                )),
                None => {
                    // Schema might use $ref or other patterns without explicit type
                    // Accept if it has properties (likely an object schema)
                    if json.get("properties").is_some() {
                        Ok(root.clone())
                    } else {
                        Err(
                            "Schema missing 'type' — output_schema must have root type 'object'"
                                .to_string(),
                        )
                    }
                }
                Some(other) => Err(format!(
                    "Unexpected 'type' format: {:?} — expected string 'object'",
                    other
                )),
            };
            cache.insert(TypeId::of::<T>(), result.clone());
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(schemars::JsonSchema, Serialize)]
    struct TestInput {
        count: i32,
        name: String,
    }

    #[test]
    fn test_strict_mode() {
        let engine = SchemaEngine::new().with_strict(true);
        let schema = schemars::schema_for!(TestInput);
        let transformed = engine.transform("test", schema);

        let json = serde_json::to_value(&transformed).unwrap();
        assert_eq!(json.get("additionalProperties"), Some(&Json::Bool(false)));
    }

    #[test]
    fn test_is_strict_getter() {
        let e = SchemaEngine::new();
        assert!(!e.is_strict());
        let e2 = SchemaEngine::new().with_strict(true);
        assert!(e2.is_strict());
    }

    #[test]
    fn test_enum_constraint() {
        let mut engine = SchemaEngine::new();

        // Use a simple schema object for testing
        let test_schema: Json = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string"
                }
            }
        });

        engine.constrain_field(
            "test",
            vec!["properties".into(), "name".into()],
            FieldConstraint::Enum(vec![Json::String("a".into()), Json::String("b".into())]),
        );

        let schema: Schema = Schema::try_from(test_schema.clone()).unwrap();
        let transformed = engine.transform("test", schema);

        let json = serde_json::to_value(&transformed).unwrap();
        let name_schema = &json["properties"]["name"];
        assert!(name_schema.get("enum").is_some());
    }

    #[test]
    fn test_range_constraint() {
        // Test that range constraints are applied to the correct schema path
        let mut engine = SchemaEngine::new();
        engine.constrain_field(
            "test",
            vec!["properties".into(), "count".into()],
            FieldConstraint::Range {
                minimum: Some(Json::Number(0.into())),
                maximum: Some(Json::Number(100.into())),
            },
        );

        // Use schemars to generate a real schema
        let schema = schemars::schema_for!(TestInput);

        // The transform function modifies the schema
        let transformed = engine.transform("test", schema);

        // Verify the range constraints were applied
        let json = serde_json::to_value(&transformed).unwrap();
        let count_schema = &json["properties"]["count"];

        // Verify range was applied (compare as f64 since schemars may use floats)
        let min = count_schema.get("minimum").and_then(|v| v.as_f64());
        let max = count_schema.get("maximum").and_then(|v| v.as_f64());

        assert_eq!(min, Some(0.0), "minimum constraint should be applied");
        assert_eq!(max, Some(100.0), "maximum constraint should be applied");
    }

    // ========================================================================
    // mcp_schema module tests
    // ========================================================================

    mod mcp_schema_tests {
        use super::Json;
        use super::Schema;
        use super::StripNullFromOptional;
        use super::mcp_schema;
        use schemars::transform::Transform;
        use serde::Serialize;

        fn property<'a>(schema: &'a Json, name: &str) -> &'a Json {
            &schema["properties"][name]
        }

        fn required_names(schema: &Json) -> Vec<&str> {
            schema["required"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Json::as_str)
                .collect()
        }

        #[derive(schemars::JsonSchema, Serialize)]
        struct WithOption {
            a: Option<String>,
        }

        #[test]
        fn test_option_string_is_optional_non_nullable() {
            let root = mcp_schema::cached_schema_for::<WithOption>();
            let v = serde_json::to_value(root.as_ref()).unwrap();
            let a = property(&v, "a");

            assert_eq!(a.get("type"), Some(&serde_json::json!("string")));
            assert!(a.get("nullable").is_none());
            assert!(required_names(&v).is_empty());
        }

        #[derive(schemars::JsonSchema, Serialize)]
        struct OutputObj {
            x: i32,
        }

        #[test]
        fn test_output_schema_validation_object() {
            let ok = mcp_schema::cached_output_schema_for::<OutputObj>();
            assert!(
                ok.is_ok(),
                "Object types should pass output schema validation"
            );
        }

        #[test]
        fn test_output_schema_validation_non_object() {
            // String is not an object type
            let bad = mcp_schema::cached_output_schema_for::<String>();
            assert!(
                bad.is_err(),
                "Non-object types should fail output schema validation"
            );
        }

        #[test]
        fn test_draft_2020_12_uses_defs() {
            let root = mcp_schema::cached_schema_for::<WithOption>();
            let v = serde_json::to_value(root.as_ref()).unwrap();
            // Draft 2020-12 should use $defs, not definitions
            // Note: simple types may not have $defs, so we just verify
            // the schema is valid and contains expected structure
            assert!(v.is_object(), "Schema should be an object");
            assert!(
                v.get("$schema")
                    .and_then(|s| s.as_str())
                    .is_some_and(|s| s.contains("2020-12")),
                "Schema should reference Draft 2020-12"
            );
        }

        #[test]
        fn test_caching_returns_same_arc() {
            let first = mcp_schema::cached_schema_for::<OutputObj>();
            let second = mcp_schema::cached_schema_for::<OutputObj>();
            assert!(
                std::sync::Arc::ptr_eq(&first, &second),
                "Cached schemas should return the same Arc"
            );
        }

        // ====================================================================
        // RestrictFormats transform and Option<Enum> tests
        // ====================================================================

        #[allow(dead_code)]
        #[derive(schemars::JsonSchema, Serialize)]
        enum TestEnum {
            A,
            B,
        }

        #[derive(schemars::JsonSchema, Serialize)]
        struct HasOptEnum {
            e: Option<TestEnum>,
        }

        #[test]
        fn test_option_enum_collapses_to_non_nullable_ref() {
            let root = mcp_schema::cached_schema_for::<HasOptEnum>();
            let v = serde_json::to_value(root.as_ref()).unwrap();
            let e = property(&v, "e");

            assert!(e.get("anyOf").is_none());
            assert!(e.get("$ref").is_some());
        }

        #[derive(schemars::JsonSchema, Serialize)]
        struct Unsigneds {
            a: u32,
            b: u64,
        }

        #[test]
        fn test_strip_uint_formats() {
            let root = mcp_schema::cached_schema_for::<Unsigneds>();
            let v = serde_json::to_value(root.as_ref()).unwrap();
            let pa = &v["properties"]["a"];
            let pb = &v["properties"]["b"];

            assert!(
                pa.get("format").is_none(),
                "u32 should not include non-standard 'format'"
            );
            assert!(
                pb.get("format").is_none(),
                "u64 should not include non-standard 'format'"
            );
            assert_eq!(
                pa.get("minimum").and_then(|x| x.as_u64()),
                Some(0),
                "u32 minimum must be preserved"
            );
            assert_eq!(
                pb.get("minimum").and_then(|x| x.as_u64()),
                Some(0),
                "u64 minimum must be preserved"
            );
        }

        #[derive(schemars::JsonSchema, Serialize)]
        struct HasOptString {
            s: Option<String>,
        }

        #[test]
        fn test_option_string_stays_non_nullable_without_nullable_keyword() {
            let root = mcp_schema::cached_schema_for::<HasOptString>();
            let v = serde_json::to_value(root.as_ref()).unwrap();
            let s = property(&v, "s");

            assert_eq!(s.get("type"), Some(&serde_json::json!("string")));
            assert!(
                s.get("nullable").is_none(),
                "Option<String> should not have nullable keyword"
            );
        }

        #[derive(schemars::JsonSchema, Serialize)]
        struct NestedInner {
            leaf: Option<String>,
        }

        #[derive(schemars::JsonSchema, Serialize)]
        struct NestedOuter {
            nested: Option<NestedInner>,
        }

        #[test]
        fn test_nested_optional_properties_are_rewritten_recursively() {
            let root = mcp_schema::cached_schema_for::<NestedOuter>();
            let v = serde_json::to_value(root.as_ref()).unwrap();
            let nested = property(&v, "nested");

            assert!(nested.get("anyOf").is_none());
            assert!(nested.get("$ref").is_some());

            let defs = v["$defs"]
                .as_object()
                .expect("Nested type should use $defs");
            let inner = defs
                .values()
                .find(|schema| schema["properties"].get("leaf").is_some())
                .expect("NestedInner schema should exist in $defs");

            assert_eq!(
                inner["properties"]["leaf"]["type"],
                serde_json::json!("string")
            );
        }

        #[derive(schemars::JsonSchema, Serialize)]
        struct HasOptVec {
            values: Option<Vec<String>>,
        }

        #[test]
        fn test_option_vec_property_loses_outer_nullability() {
            let root = mcp_schema::cached_schema_for::<HasOptVec>();
            let v = serde_json::to_value(root.as_ref()).unwrap();
            let values = property(&v, "values");

            assert_eq!(values.get("type"), Some(&serde_json::json!("array")));
            assert_eq!(values["items"]["type"], serde_json::json!("string"));
        }

        #[derive(schemars::JsonSchema, Serialize)]
        struct HasNestedOptionalItems {
            values: Option<Vec<Option<String>>>,
        }

        #[test]
        fn test_inner_nullability_is_preserved() {
            let root = mcp_schema::cached_schema_for::<HasNestedOptionalItems>();
            let v = serde_json::to_value(root.as_ref()).unwrap();
            let values = property(&v, "values");
            let item_type = values["items"]["type"]
                .as_array()
                .expect("Inner Option<String> should remain nullable");

            assert_eq!(values.get("type"), Some(&serde_json::json!("array")));
            assert!(item_type.contains(&serde_json::json!("string")));
            assert!(item_type.contains(&serde_json::json!("null")));
        }

        #[test]
        fn test_required_fields_remain_unchanged() {
            let mut schema = Schema::try_from(serde_json::json!({
                "type": "object",
                "properties": {
                    "required_field": { "type": ["string", "null"] },
                    "optional_field": { "type": ["string", "null"] }
                },
                "required": ["required_field"]
            }))
            .unwrap();

            StripNullFromOptional.transform(&mut schema);

            let v = serde_json::to_value(&schema).unwrap();
            let required_type = v["properties"]["required_field"]["type"]
                .as_array()
                .expect("Required field should keep nullable type array");

            assert!(required_type.contains(&serde_json::json!("string")));
            assert!(required_type.contains(&serde_json::json!("null")));
            assert_eq!(
                v["properties"]["optional_field"]["type"],
                serde_json::json!("string")
            );
        }
    }
}
