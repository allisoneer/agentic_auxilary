//! MCP tool schema renderer.

use schemars::Schema;
use serde_json::{Value, json};

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
    let inp = serde_json::to_value(input_schema).expect("Schema serialization must succeed");
    let mut obj = json!({
        "name": name,
        "description": description,
        "inputSchema": inp,
    });

    if let Some(out) = output_schema {
        let out_val = serde_json::to_value(out).expect("Schema serialization must succeed");
        obj.as_object_mut()
            .unwrap()
            .insert("outputSchema".into(), out_val);
    }

    obj
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(schemars::JsonSchema)]
    #[allow(dead_code)]
    struct TestInput {
        path: String,
    }

    #[derive(schemars::JsonSchema)]
    #[allow(dead_code)]
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
