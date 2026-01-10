//! OpenAI function calling schema renderer.

use schemars::schema::RootSchema;
use serde_json::{Value, json};

/// Render a tool as an OpenAI function definition.
///
/// Output format:
/// ```json
/// {
///   "type": "function",
///   "function": {
///     "name": "...",
///     "description": "...",
///     "strict": true,
///     "parameters": { ... }
///   }
/// }
/// ```
pub fn render_function(
    name: &str,
    description: &str,
    parameters: &RootSchema,
    strict: bool,
) -> Value {
    let params = serde_json::to_value(parameters).unwrap_or(json!({"type": "object"}));
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "strict": strict,
            "parameters": params
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(schemars::JsonSchema)]
    #[allow(dead_code)]
    struct TestInput {
        message: String,
    }

    #[test]
    fn test_render_function() {
        let schema = schemars::schema_for!(TestInput);
        let rendered = render_function("greet", "Greet someone", &schema, true);

        assert_eq!(rendered["type"], "function");
        assert_eq!(rendered["function"]["name"], "greet");
        assert_eq!(rendered["function"]["description"], "Greet someone");
        assert_eq!(rendered["function"]["strict"], true);
        assert!(rendered["function"]["parameters"].is_object());
    }

    #[test]
    fn test_render_function_non_strict() {
        let schema = schemars::schema_for!(TestInput);
        let rendered = render_function("greet", "Greet someone", &schema, false);

        assert_eq!(rendered["function"]["strict"], false);
    }
}
