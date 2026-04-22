//! Skill types for `opencode_rs`.

use serde::Deserialize;
use serde::Serialize;

/// A skill definition returned by `GET /skill`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Skill identifier/name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Absolute source location of the skill file.
    pub location: String,
    /// Markdown content for the skill.
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_deserialize() {
        let json = r#"{
            "name": "code-review",
            "description": "Review code for issues",
            "location": "/skills/code-review/SKILL.md",
            "content": "Please review the following code..."
        }"#;
        let skill: Skill = serde_json::from_str(json).unwrap();
        assert_eq!(skill.name, "code-review");
        assert_eq!(skill.description, "Review code for issues");
        assert_eq!(skill.location, "/skills/code-review/SKILL.md");
        assert_eq!(skill.content, "Please review the following code...");
    }
}
