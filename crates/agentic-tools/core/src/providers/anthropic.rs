//! Anthropic tool schema renderer.

use schemars::Schema;
use serde_json::{Value, json};

/// Render a tool as an Anthropic tool definition.
///
/// Output format:
/// ```json
/// {
///   "name": "...",
///   "description": "...",
///   "strict": true,
///   "input_schema": { ... }
/// }
/// ```
pub fn render_tool(name: &str, description: &str, input_schema: &Schema, strict: bool) -> Value {
    let schema = serde_json::to_value(input_schema).expect("Schema serialization must succeed");
    json!({
        "name": name,
        "description": description,
        "strict": strict,
        "input_schema": schema
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(schemars::JsonSchema)]
    #[allow(dead_code)]
    struct TestInput {
        query: String,
    }

    #[test]
    fn test_render_tool() {
        let schema = schemars::schema_for!(TestInput);
        let rendered = render_tool("search", "Search for something", &schema, true);

        assert_eq!(rendered["name"], "search");
        assert_eq!(rendered["description"], "Search for something");
        assert_eq!(rendered["strict"], true);
        assert!(rendered["input_schema"].is_object());
    }

    #[test]
    fn test_render_tool_non_strict() {
        let schema = schemars::schema_for!(TestInput);
        let rendered = render_tool("search", "Search for something", &schema, false);

        assert_eq!(rendered["strict"], false);
    }
}
