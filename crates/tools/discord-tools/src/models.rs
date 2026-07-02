use crate::DiscordToolsError;
use agentic_tools_core::fmt::TextFormat;
use agentic_tools_core::fmt::TextOptions;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::fmt::Write as _;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DiscordSearchMessagesInput {
    pub query: String,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscordMessageHit {
    pub message_id: String,
    pub channel_id: String,
    pub author_id: Option<String>,
    pub author_username: Option<String>,
    pub timestamp: Option<String>,
    pub snippet: String,
    pub jump_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscordSearchMessagesOutput {
    pub guild_id: String,
    pub query: String,
    pub limit: u32,
    pub offset: u32,
    pub results: Vec<DiscordMessageHit>,
    pub shown: u32,
    pub total_results: Option<u64>,
    pub has_more: bool,
    pub next_offset: Option<u32>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl DiscordSearchMessagesOutput {
    pub fn from_search_response(
        guild_id: u64,
        query: String,
        limit: u32,
        offset: u32,
        warnings: Vec<String>,
        json: &Value,
    ) -> Result<Self, DiscordToolsError> {
        let messages = json
            .get("messages")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                DiscordToolsError::External("discord search response missing messages array".into())
            })?;

        let results = messages
            .iter()
            .flat_map(flatten_message_group)
            .filter_map(|message| parse_message_hit(guild_id, message))
            .collect::<Vec<_>>();

        let shown = results.len() as u32;
        let total_results = json.get("total_results").and_then(value_to_u64);
        let has_more = total_results.map_or(shown == limit, |total| {
            u64::from(offset) + u64::from(shown) < total
        });
        let next_offset = has_more.then_some(offset.saturating_add(shown));

        Ok(Self {
            guild_id: guild_id.to_string(),
            query,
            limit,
            offset,
            results,
            shown,
            total_results,
            has_more,
            next_offset,
            warnings,
        })
    }
}

impl TextFormat for DiscordSearchMessagesOutput {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Discord search: {:?} (guild {}, offset {}, limit {})",
            self.query, self.guild_id, self.offset, self.limit
        );

        if self.results.is_empty() {
            let _ = writeln!(out, "Results: <none>");
        } else {
            let _ = writeln!(out, "Results:");
            for (index, hit) in self.results.iter().enumerate() {
                let _ = writeln!(out, "  {}. {} — {}", index + 1, hit.snippet, hit.jump_url);
            }
        }

        if let Some(next_offset) = self.next_offset {
            let _ = writeln!(
                out,
                "\n(next_offset={next_offset}, has_more={})",
                self.has_more
            );
        }

        if !self.warnings.is_empty() {
            let _ = writeln!(out, "\nWarnings:");
            for warning in &self.warnings {
                let _ = writeln!(out, "  - {warning}");
            }
        }

        out
    }
}

fn flatten_message_group(value: &Value) -> Box<dyn Iterator<Item = &Value> + '_> {
    if let Some(group) = value.as_array() {
        Box::new(group.iter())
    } else {
        Box::new(std::iter::once(value))
    }
}

fn parse_message_hit(guild_id: u64, message: &Value) -> Option<DiscordMessageHit> {
    let object = message.as_object()?;
    let message_id = object.get("id").and_then(value_to_string)?;
    let channel_id = object.get("channel_id").and_then(value_to_string)?;
    let author = object.get("author").and_then(Value::as_object);
    let author_id = author
        .and_then(|author| author.get("id"))
        .and_then(value_to_string);
    let author_username = author
        .and_then(|author| author.get("username"))
        .and_then(value_to_string);
    let timestamp = object.get("timestamp").and_then(value_to_string);
    let snippet = object
        .get("content")
        .and_then(value_to_string)
        .unwrap_or_default();

    Some(DiscordMessageHit {
        jump_url: format!("https://discord.com/channels/{guild_id}/{channel_id}/{message_id}"),
        message_id,
        channel_id,
        author_id,
        author_username,
        timestamp,
        snippet,
    })
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_message_array_of_arrays() {
        let json = serde_json::json!({
            "total_results": 1,
            "messages": [[{
                "id": "555",
                "channel_id": "777",
                "content": "hello world",
                "timestamp": "2026-01-01T00:00:00.000Z",
                "author": {"id": "999", "username": "alice"}
            }]]
        });

        let output = DiscordSearchMessagesOutput::from_search_response(
            123,
            "hello".into(),
            10,
            0,
            vec![],
            &json,
        )
        .unwrap();

        assert_eq!(output.results.len(), 1);
        assert_eq!(
            output.results[0].jump_url,
            "https://discord.com/channels/123/777/555"
        );
        assert_eq!(output.total_results, Some(1));
        assert!(!output.has_more);
    }

    #[test]
    fn parses_message_array_of_objects() {
        let json = serde_json::json!({
            "messages": [{
                "id": "1",
                "channel_id": "2",
                "content": "hello"
            }]
        });

        let output = DiscordSearchMessagesOutput::from_search_response(
            99,
            "hello".into(),
            1,
            0,
            vec![],
            &json,
        )
        .unwrap();

        assert_eq!(output.results[0].message_id, "1");
        assert_eq!(output.results[0].channel_id, "2");
    }

    #[test]
    fn computes_next_offset_when_total_results_more_than_page() {
        let json = serde_json::json!({
            "total_results": 10,
            "messages": [{
                "id": "1",
                "channel_id": "2",
                "content": "hello"
            }]
        });

        let output = DiscordSearchMessagesOutput::from_search_response(
            99,
            "hello".into(),
            1,
            3,
            vec![],
            &json,
        )
        .unwrap();

        assert!(output.has_more);
        assert_eq!(output.next_offset, Some(4));
    }
}
