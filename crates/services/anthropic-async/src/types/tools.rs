//! Tool calling support for Anthropic API
//!
//! This module provides types for defining tools that Claude can call.
//!
//! # Example (with schemars feature)
//!
//! ```rust,no_run
//! # #[cfg(feature = "schemars")]
//! # {
//! use anthropic_async::types::tools;
//! use serde::{Serialize, Deserialize};
//! use schemars::JsonSchema;
//!
//! // Define tools as an enum with adjacently tagged format
//! #[derive(Serialize, Deserialize, JsonSchema)]
//! #[serde(tag = "action", content = "params", rename_all = "snake_case")]
//! enum Actions {
//!     SendEmail { to: String, subject: String },
//!     SearchWeb { query: String },
//! }
//!
//! // Generate tool definition from schema
//! let tool = tools::schema::tool_from_schema::<Actions>(
//!     "actions",
//!     Some("Available actions")
//! );
//!
//! // Parse tool use response back to typed enum
//! # use anthropic_async::types::ContentBlock;
//! let tool_use = ContentBlock::ToolUse {
//!     id: "123".into(),
//!     name: "send_email".into(),
//!     input: serde_json::json!({ "to": "user@example.com", "subject": "Hello" })
//! };
//!
//! if let ContentBlock::ToolUse { name, input, .. } = tool_use {
//!     let action = tools::schema::try_parse_tool_use::<Actions>(&name, &input).unwrap();
//!     // action is now typed as Actions enum
//! }
//! # }
//! ```

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
    /// Enable strict mode for tool input validation (beta)
    ///
    /// When enabled, tool inputs must exactly match the schema with no additional properties.
    /// Requires a structured outputs beta header to be enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
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
    use schemars::JsonSchema;

    use super::Tool;

    /// Generate a Tool definition from a type implementing `JsonSchema`
    ///
    /// # Panics
    /// Panics if the schema cannot be serialized to JSON (should never happen with valid schemas)
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
        let schema = schemars::schema_for!(T);
        let schema_value = serde_json::to_value(&schema).expect("valid schema");
        Tool {
            name: name.to_string(),
            description: description.map(std::string::ToString::to_string),
            input_schema: schema_value,
            cache_control: None,
            strict: None,
        }
    }

    /// Parse tool use response back to typed enum
    ///
    /// # Errors
    /// Returns error if the input cannot be deserialized to type T
    pub fn try_parse_tool_use<T: serde::de::DeserializeOwned>(
        name: &str,
        input: &serde_json::Value,
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
            strict: None,
        };
        let s = serde_json::to_string(&tool).unwrap();
        assert!(s.contains(r#""name":"calculator""#));
        assert!(s.contains(r#""description":"Math tool""#));
        assert!(s.contains(r#""input_schema""#));
        // strict should not appear when None
        assert!(!s.contains("strict"));
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
