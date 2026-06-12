use serde_json::Map;
use serde_json::Value;

const DEFS_PREFIX: &str = "#/$defs/";
const DEFINITIONS_PREFIX: &str = "#/definitions/";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SchemaPublicationProfile {
    #[default]
    Canonical,
    InlineLocalRefs,
}

pub fn apply_schema_publication_profile(profile: SchemaPublicationProfile, schema: &mut Value) {
    match profile {
        SchemaPublicationProfile::Canonical => {}
        SchemaPublicationProfile::InlineLocalRefs => {
            let snapshot = schema.clone();
            inline_local_refs(schema, &snapshot, &mut Vec::new());
        }
    }
}

fn inline_local_refs(node: &mut Value, snapshot: &Value, stack: &mut Vec<String>) {
    if let Some(ref_path) = local_ref_path(node)
        && !stack.contains(&ref_path)
        && let Some(mut expanded) = expand_ref(&ref_path, snapshot, stack)
    {
        let siblings = ref_siblings(node);
        merge_ref_siblings(&mut expanded, siblings);
        *node = expanded;
        return;
    }

    match node {
        Value::Object(object) => {
            for value in object.values_mut() {
                inline_local_refs(value, snapshot, stack);
            }
        }
        Value::Array(values) => {
            for value in values {
                inline_local_refs(value, snapshot, stack);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn local_ref_path(node: &Value) -> Option<String> {
    let ref_value = node.as_object()?.get("$ref")?.as_str()?;
    if ref_value.starts_with(DEFS_PREFIX) || ref_value.starts_with(DEFINITIONS_PREFIX) {
        Some(ref_value.to_string())
    } else {
        None
    }
}

fn expand_ref(ref_path: &str, snapshot: &Value, stack: &mut Vec<String>) -> Option<Value> {
    let pointer = ref_path.strip_prefix('#')?;
    let target = snapshot.pointer(pointer)?;

    stack.push(ref_path.to_string());
    let mut expanded = target.clone();
    inline_local_refs(&mut expanded, snapshot, stack);
    stack.pop();

    Some(expanded)
}

fn ref_siblings(node: &Value) -> Map<String, Value> {
    node.as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter(|(key, _)| key.as_str() != "$ref")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn merge_ref_siblings(expanded: &mut Value, siblings: Map<String, Value>) {
    if siblings.is_empty() {
        return;
    }

    let Some(expanded_object) = expanded.as_object_mut() else {
        return;
    };

    for (key, value) in siblings {
        expanded_object.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::SchemaPublicationProfile;
    use super::apply_schema_publication_profile;
    use serde_json::Value;

    fn collect_local_refs<'a>(node: &'a Value, refs: &mut Vec<&'a str>) {
        match node {
            Value::Object(object) => {
                if let Some(ref_value) = object.get("$ref").and_then(Value::as_str)
                    && (ref_value.starts_with("#/$defs/")
                        || ref_value.starts_with("#/definitions/"))
                {
                    refs.push(ref_value);
                }

                for value in object.values() {
                    collect_local_refs(value, refs);
                }
            }
            Value::Array(values) => {
                for value in values {
                    collect_local_refs(value, refs);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }

    #[test]
    fn canonical_profile_is_a_no_op() {
        let original = serde_json::json!({
            "type": "object",
            "properties": {
                "item": { "$ref": "#/$defs/Item" }
            },
            "$defs": {
                "Item": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    }
                }
            }
        });
        let mut actual = original.clone();

        apply_schema_publication_profile(SchemaPublicationProfile::Canonical, &mut actual);

        assert_eq!(actual, original);
    }

    #[test]
    fn inline_local_refs_inlines_defs_refs() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "item": { "$ref": "#/$defs/Item" }
            },
            "$defs": {
                "Item": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    },
                    "required": ["name"]
                }
            }
        });

        apply_schema_publication_profile(SchemaPublicationProfile::InlineLocalRefs, &mut schema);

        assert_eq!(
            schema["properties"]["item"],
            serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"]
            })
        );
    }

    #[test]
    fn inline_local_refs_inlines_definitions_refs() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "item": { "$ref": "#/definitions/Item" }
            },
            "definitions": {
                "Item": {
                    "type": "string",
                    "enum": ["a", "b"]
                }
            }
        });

        apply_schema_publication_profile(SchemaPublicationProfile::InlineLocalRefs, &mut schema);

        assert_eq!(
            schema["properties"]["item"],
            serde_json::json!({
                "type": "string",
                "enum": ["a", "b"]
            })
        );
    }

    #[test]
    fn inline_local_refs_preserves_nullable_semantics() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "maybe_item": { "$ref": "#/$defs/MaybeItem" }
            },
            "$defs": {
                "MaybeItem": {
                    "anyOf": [
                        { "type": "null" },
                        {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" }
                            }
                        }
                    ]
                }
            }
        });

        apply_schema_publication_profile(SchemaPublicationProfile::InlineLocalRefs, &mut schema);

        assert_eq!(
            schema["properties"]["maybe_item"]["anyOf"],
            serde_json::json!([
                { "type": "null" },
                {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    }
                }
            ])
        );
    }

    #[test]
    fn inline_local_refs_leaves_cyclic_local_refs_unresolved() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "node": { "$ref": "#/$defs/Node" }
            },
            "$defs": {
                "Node": {
                    "type": "object",
                    "properties": {
                        "next": { "$ref": "#/$defs/Node" }
                    }
                }
            }
        });

        apply_schema_publication_profile(SchemaPublicationProfile::InlineLocalRefs, &mut schema);

        let mut refs = Vec::new();
        collect_local_refs(&schema, &mut refs);
        assert!(
            refs.iter().all(|ref_value| *ref_value == "#/$defs/Node"),
            "only the cyclic local ref should remain unresolved"
        );
        assert_eq!(
            schema["properties"]["node"]["type"],
            serde_json::json!("object")
        );
        assert_eq!(
            schema["properties"]["node"]["properties"]["next"]["$ref"],
            serde_json::json!("#/$defs/Node")
        );
    }
}
