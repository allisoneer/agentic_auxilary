use serde::{Deserialize, Serialize};

use super::common::CacheControl;

/// Tool definition for Claude to use
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tool {
    /// Tool name
    pub name: String,
    /// Optional tool description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON schema for tool input
    pub input_schema: serde_json::Value,
    /// Optional cache control for prompt caching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Tool choice strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    /// Let Claude decide whether to use tools
    Auto {
        /// Disable parallel tool use
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// Force Claude to use at least one tool
    Any {
        /// Disable parallel tool use
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// Disable tool use
    #[serde(rename = "none")]
    None,
    /// Force Claude to use a specific tool
    Tool {
        /// Name of the tool to use
        name: String,
        /// Disable parallel tool use
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
}

impl Default for ToolChoice {
    fn default() -> Self {
        Self::Auto {
            disable_parallel_tool_use: None,
        }
    }
}

/// Type-safe tool schema generation (requires schemars feature)
#[cfg(feature = "schemars")]
pub mod schema {
    use super::*;
    use schemars::JsonSchema;

    /// Generate a Tool definition from a type implementing JsonSchema
    ///
    /// # Example
    /// ```ignore
    /// use schemars::JsonSchema;
    /// use serde::{Serialize, Deserialize};
    ///
    /// #[derive(Serialize, Deserialize, JsonSchema)]
    /// #[serde(tag = "action", content = "params")]
    /// enum MyTools {
    ///     GetWeather { city: String },
    ///     GetTime { timezone: String },
    /// }
    ///
    /// let tool = tool_from_schema::<MyTools>("my_tools", Some("Weather and time tools"));
    /// ```
    #[must_use]
    pub fn tool_from_schema<T: JsonSchema>(name: &str, description: Option<&str>) -> Tool {
        let root = schemars::schema_for!(T);
        let schema_value = serde_json::to_value(root.schema).expect("valid schema");
        Tool {
            name: name.to_string(),
            description: description.map(std::string::ToString::to_string),
            input_schema: schema_value,
            cache_control: None,
        }
    }

    /// Parse tool use response back to typed enum
    ///
    /// # Errors
    /// Returns error if the input cannot be deserialized to type T
    pub fn try_parse_tool_use<T: serde::de::DeserializeOwned>(
        name: &str,
        input: serde_json::Value,
    ) -> serde_json::Result<T> {
        let wrapped = serde_json::json!({
            "action": name,
            "params": input
        });
        serde_json::from_value(wrapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_choice_auto_ser() {
        let tc = ToolChoice::Auto {
            disable_parallel_tool_use: None,
        };
        let s = serde_json::to_string(&tc).unwrap();
        assert!(s.contains(r#""type":"auto""#));
    }

    #[test]
    fn tool_choice_any_ser() {
        let tc = ToolChoice::Any {
            disable_parallel_tool_use: Some(true),
        };
        let s = serde_json::to_string(&tc).unwrap();
        assert!(s.contains(r#""type":"any""#));
        assert!(s.contains(r#""disable_parallel_tool_use":true"#));
    }

    #[test]
    fn tool_choice_none_ser() {
        let tc = ToolChoice::None;
        let s = serde_json::to_string(&tc).unwrap();
        assert_eq!(s, r#"{"type":"none"}"#);
    }

    #[test]
    fn tool_choice_tool_ser() {
        let tc = ToolChoice::Tool {
            name: "get_weather".into(),
            disable_parallel_tool_use: None,
        };
        let s = serde_json::to_string(&tc).unwrap();
        assert!(s.contains(r#""type":"tool""#));
        assert!(s.contains(r#""name":"get_weather""#));
    }

    #[test]
    fn tool_ser() {
        let tool = Tool {
            name: "calculator".into(),
            description: Some("Math tool".into()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "expression": { "type": "string" }
                }
            }),
            cache_control: None,
        };
        let s = serde_json::to_string(&tool).unwrap();
        assert!(s.contains(r#""name":"calculator""#));
        assert!(s.contains(r#""description":"Math tool""#));
        assert!(s.contains(r#""input_schema""#));
    }

    #[cfg(feature = "schemars")]
    #[test]
    fn schema_tool_generation() {
        use schemars::JsonSchema;

        #[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
        #[serde(tag = "action", content = "params")]
        enum TestTools {
            Echo { message: String },
        }

        let tool = schema::tool_from_schema::<TestTools>("test", Some("Test tool"));
        assert_eq!(tool.name, "test");
        assert_eq!(tool.description, Some("Test tool".into()));
        assert!(tool.input_schema.is_object());
    }
}
