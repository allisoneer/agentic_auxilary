//! MCP tool schema renderer.

use schemars::Schema;
use serde_json::Map;
use serde_json::Value;

/// Render a tool as an MCP tool definition.
///
/// Output format:
/// ```json
/// {
///   "name": "...",
///   "description": "...",
///   "inputSchema": { ... },
///   "outputSchema": { ... }  // optional
/// }
/// ```
pub fn render_tool(
    name: &str,
    description: &str,
    input_schema: &Schema,
    output_schema: Option<&Schema>,
) -> Value {
    let inp = match serde_json::to_value(input_schema) {
        Ok(value) => value,
        Err(error) => panic!("Schema serialization must succeed: {error}"),
    };
    let mut obj = Map::from_iter([
        ("name".into(), Value::String(name.to_string())),
        ("description".into(), Value::String(description.to_string())),
        ("inputSchema".into(), inp),
    ]);

    if let Some(out) = output_schema {
        let out_val = match serde_json::to_value(out) {
            Ok(value) => value,
            Err(error) => panic!("Schema serialization must succeed: {error}"),
        };
        obj.insert("outputSchema".into(), out_val);
    }

    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(schemars::JsonSchema)]
    #[expect(dead_code)]
    struct TestInput {
        path: String,
    }

    #[derive(schemars::JsonSchema)]
    #[expect(dead_code)]
    struct TestOutput {
        content: String,
    }

    #[test]
    fn test_render_tool_without_output() {
        let input_schema = schemars::schema_for!(TestInput);
        let rendered = render_tool("read_file", "Read a file", &input_schema, None);

        assert_eq!(rendered["name"], "read_file");
        assert_eq!(rendered["description"], "Read a file");
        assert!(rendered["inputSchema"].is_object());
        assert!(rendered.get("outputSchema").is_none());
    }

    #[test]
    fn test_render_tool_with_output() {
        let input_schema = schemars::schema_for!(TestInput);
        let output_schema = schemars::schema_for!(TestOutput);
        let rendered = render_tool(
            "read_file",
            "Read a file",
            &input_schema,
            Some(&output_schema),
        );

        assert_eq!(rendered["name"], "read_file");
        assert!(rendered["inputSchema"].is_object());
        assert!(rendered["outputSchema"].is_object());
    }
}
