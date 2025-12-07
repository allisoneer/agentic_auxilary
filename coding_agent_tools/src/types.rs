use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::{InstanceType, Metadata, NumberValidation, Schema, SchemaObject};
use serde::{Deserialize, Serialize};
use universal_tool_core::mcp::McpFormatter;

/// Agent type determines the model and behavior characteristics.
/// - Locator: Fast discovery (haiku), finds WHERE things are
/// - Analyzer: Deep analysis (sonnet), understands HOW things work
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    #[default]
    Locator,
    Analyzer,
}

/// Agent location determines the working context and available tools.
/// - Codebase: Current repository (code, configs, tests)
/// - Thoughts: Thought documents in active branch
/// - References: Cloned reference repositories
/// - Web: Internet search (no working directory)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentLocation {
    #[default]
    Codebase,
    Thoughts,
    References,
    Web,
}

/// Output from spawn_agent tool - plain text response from the subagent.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentOutput {
    pub text: String,
}

impl AgentOutput {
    pub fn new(text: String) -> Self {
        Self { text }
    }
}

impl McpFormatter for AgentOutput {
    fn mcp_format_text(&self) -> String {
        self.text.clone()
    }
}

/// Depth of directory traversal (0-10).
/// - 0: Header only (just the directory path)
/// - 1: Immediate children only (like `ls`)
/// - 2-10: Tree up to N levels deep
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct Depth(u8);

impl Depth {
    /// Maximum allowed depth value
    pub const MAX: u8 = 10;

    /// Create a new Depth, returning an error if value exceeds MAX
    pub fn new(v: u8) -> Result<Self, String> {
        if v <= Self::MAX {
            Ok(Self(v))
        } else {
            Err(format!("Depth {} exceeds max {}", v, Self::MAX))
        }
    }

    /// Get the raw depth value
    pub fn as_u8(self) -> u8 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Depth {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = u8::deserialize(deserializer)?;
        Depth::new(v).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Depth {
    fn schema_name() -> String {
        "Depth0to10".into()
    }

    fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            metadata: Some(Box::new(Metadata {
                description: Some("Depth of directory traversal (0-10)".into()),
                ..Default::default()
            })),
            instance_type: Some(InstanceType::Integer.into()),
            number: Some(Box::new(NumberValidation {
                minimum: Some(0.0),
                maximum: Some(10.0),
                ..Default::default()
            })),
            ..Default::default()
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum Show {
    #[default]
    All,
    Files,
    Dirs,
}

impl std::str::FromStr for Show {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(Show::All),
            "files" => Ok(Show::Files),
            "dirs" | "directories" => Ok(Show::Dirs),
            _ => Err(format!("invalid show: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LsEntry {
    pub path: String,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LsOutput {
    pub root: String,
    pub entries: Vec<LsEntry>,
    pub has_more: bool,
    pub warnings: Vec<String>,
}

// =============================================================================
// Truncation info sentinel for carrying pagination stats in warnings
// =============================================================================

/// Hidden sentinel prefix for pagination info in warnings.
pub const TRUNCATION_SENTINEL: &str = "<<<mcp:ls:page_info>>>";

/// Encode truncation info into a sentinel warning string.
pub fn encode_truncation_info(shown: usize, total: usize, page_size: usize) -> String {
    format!(
        "{} shown={} total={} page_size={}",
        TRUNCATION_SENTINEL, shown, total, page_size
    )
}

/// Decode truncation info from a sentinel warning string.
/// Returns (shown, total, page_size) if valid, None otherwise.
fn decode_truncation_info(s: &str) -> Option<(usize, usize, usize)> {
    if !s.starts_with(TRUNCATION_SENTINEL) {
        return None;
    }

    let mut shown = None;
    let mut total = None;
    let mut page_size = None;

    for part in s.split_whitespace() {
        if let Some(val) = part.strip_prefix("shown=") {
            shown = val.parse::<usize>().ok();
        } else if let Some(val) = part.strip_prefix("total=") {
            total = val.parse::<usize>().ok();
        } else if let Some(val) = part.strip_prefix("page_size=") {
            page_size = val.parse::<usize>().ok();
        }
    }

    match (shown, total, page_size) {
        (Some(a), Some(b), Some(c)) => Some((a, b, c)),
        _ => None,
    }
}

impl McpFormatter for LsOutput {
    fn mcp_format_text(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();

        // Header: absolute canonical path with trailing /
        let _ = writeln!(out, "{}/", self.root.trim_end_matches('/'));

        // Body: 2-space indent, directories with trailing /
        for entry in &self.entries {
            let _ = write!(out, "  {}", entry.path);
            if matches!(entry.kind, EntryKind::Dir) && !entry.path.ends_with('/') {
                out.push('/');
            }
            out.push('\n');
        }

        // Separate truncation sentinel from normal warnings
        let mut trunc_info: Option<(usize, usize, usize)> = None;
        let mut normal_warnings: Vec<&str> = Vec::new();
        for w in &self.warnings {
            if let Some(info) = decode_truncation_info(w) {
                trunc_info = Some(info);
            } else {
                normal_warnings.push(w);
            }
        }

        // Truncation footer (for MCP pagination)
        if self.has_more {
            if let Some((shown, total, page_size)) = trunc_info {
                let remaining = total.saturating_sub(shown);
                let pages_remaining = remaining.div_ceil(page_size);
                let _ = writeln!(
                    out,
                    "(truncated — showing {} of {} entries; {} page{} remaining; call again with same params for next page{})",
                    shown,
                    total,
                    pages_remaining,
                    if pages_remaining == 1 { "" } else { "s" },
                    if pages_remaining > 1 {
                        "\nREMINDER: You can also narrow your search with additional param filters if desired"
                    } else {
                        ""
                    }
                );
            } else {
                // Fallback for tests that construct LsOutput manually without sentinel
                let _ = writeln!(
                    out,
                    "(truncated — call again with same params for next page)"
                );
            }
        }

        // Normal warnings footer
        for warning in normal_warnings {
            let _ = writeln!(out, "Note: {}", warning);
        }

        out.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_default() {
        let default = AgentType::default();
        assert_eq!(default, AgentType::Locator);
    }

    #[test]
    fn test_agent_location_default() {
        let default = AgentLocation::default();
        assert_eq!(default, AgentLocation::Codebase);
    }

    #[test]
    fn test_agent_type_serde_roundtrip() {
        for agent_type in [AgentType::Locator, AgentType::Analyzer] {
            let json = serde_json::to_string(&agent_type).unwrap();
            let deserialized: AgentType = serde_json::from_str(&json).unwrap();
            assert_eq!(agent_type, deserialized);
        }
    }

    #[test]
    fn test_agent_location_serde_roundtrip() {
        for location in [
            AgentLocation::Codebase,
            AgentLocation::Thoughts,
            AgentLocation::References,
            AgentLocation::Web,
        ] {
            let json = serde_json::to_string(&location).unwrap();
            let deserialized: AgentLocation = serde_json::from_str(&json).unwrap();
            assert_eq!(location, deserialized);
        }
    }

    #[test]
    fn test_agent_type_snake_case_serialization() {
        assert_eq!(
            serde_json::to_string(&AgentType::Locator).unwrap(),
            "\"locator\""
        );
        assert_eq!(
            serde_json::to_string(&AgentType::Analyzer).unwrap(),
            "\"analyzer\""
        );
    }

    #[test]
    fn test_agent_location_snake_case_serialization() {
        assert_eq!(
            serde_json::to_string(&AgentLocation::Codebase).unwrap(),
            "\"codebase\""
        );
        assert_eq!(
            serde_json::to_string(&AgentLocation::Thoughts).unwrap(),
            "\"thoughts\""
        );
        assert_eq!(
            serde_json::to_string(&AgentLocation::References).unwrap(),
            "\"references\""
        );
        assert_eq!(
            serde_json::to_string(&AgentLocation::Web).unwrap(),
            "\"web\""
        );
    }
}
