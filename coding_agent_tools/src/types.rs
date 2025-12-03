use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::{InstanceType, Metadata, NumberValidation, Schema, SchemaObject};
use serde::{Deserialize, Serialize};
use universal_tool_core::mcp::McpFormatter;

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

        // Truncation footer (for MCP pagination)
        if self.has_more {
            let _ = writeln!(
                out,
                "(truncated — call again with same params for next page; for deep trees consider listing a subdirectory or using show='files'|'dirs')"
            );
        }

        // Warnings footer
        for warning in &self.warnings {
            let _ = writeln!(out, "Note: {}", warning);
        }

        out.trim_end().to_string()
    }
}
