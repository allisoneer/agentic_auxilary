//! Skill types for `opencode_rs`.
//!
//! Types for skills (reusable prompt templates/workflows).

use serde::Deserialize;
use serde::Serialize;

/// A skill definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    /// Skill identifier/name.
    pub name: String,

    /// Skill description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Skill file path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Whether this is a built-in skill.
    #[serde(default)]
    pub builtin: bool,

    /// Skill content/template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Response for skill directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDirs {
    /// List of directories containing skills.
    #[serde(default)]
    pub dirs: Vec<String>,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Request to get a specific skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGetRequest {
    /// Skill name to retrieve.
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_info_minimal() {
        let json = r#"{"name": "code-review"}"#;
        let skill: SkillInfo = serde_json::from_str(json).unwrap();
        assert_eq!(skill.name, "code-review");
        assert!(skill.description.is_none());
        assert!(!skill.builtin);
    }

    #[test]
    fn test_skill_info_full() {
        let json = r#"{
            "name": "code-review",
            "description": "Review code for issues",
            "path": "/skills/code-review.md",
            "builtin": true,
            "content": "Please review the following code..."
        }"#;
        let skill: SkillInfo = serde_json::from_str(json).unwrap();
        assert_eq!(skill.name, "code-review");
        assert_eq!(
            skill.description,
            Some("Review code for issues".to_string())
        );
        assert_eq!(skill.path, Some("/skills/code-review.md".to_string()));
        assert!(skill.builtin);
        assert!(skill.content.is_some());
    }

    #[test]
    fn test_skill_dirs() {
        let json = r#"{"dirs": ["/project/.opencode/skills", "/home/user/.opencode/skills"]}"#;
        let dirs: SkillDirs = serde_json::from_str(json).unwrap();
        assert_eq!(dirs.dirs.len(), 2);
    }

    #[test]
    fn test_skill_extra_fields_preserved() {
        let json = r#"{
            "name": "test-skill",
            "futureField": "value"
        }"#;
        let skill: SkillInfo = serde_json::from_str(json).unwrap();
        assert_eq!(skill.extra["futureField"], "value");
    }
}
