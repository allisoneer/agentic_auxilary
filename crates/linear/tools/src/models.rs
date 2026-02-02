use agentic_tools_core::fmt::{TextFormat, TextOptions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Truncate a string to at most `max` characters (UTF-8 safe).
fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

// ============================================================================
// Nested Ref types for structured JSON output
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserRef {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TeamRef {
    pub id: String,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowStateRef {
    pub id: String,
    pub name: String,
    pub state_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ParentIssueRef {
    pub id: String,
    pub identifier: String,
}

// ============================================================================
// Issue models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueSummary {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub url: String,

    pub team: TeamRef,
    pub state: Option<WorkflowStateRef>,
    pub assignee: Option<UserRef>,
    pub creator: Option<UserRef>,
    pub project: Option<ProjectRef>,

    pub priority: i32,
    pub priority_label: String,

    pub label_ids: Vec<String>,
    pub due_date: Option<String>,

    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    pub issues: Vec<IssueSummary>,
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueDetails {
    pub issue: IssueSummary,
    pub description: Option<String>,

    pub estimate: Option<f64>,
    pub parent: Option<ParentIssueRef>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub canceled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateIssueResult {
    pub success: bool,
    pub issue: Option<IssueSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommentResult {
    pub success: bool,
    pub comment_id: Option<String>,
    pub body: Option<String>,
    pub created_at: Option<String>,
}

// ============================================================================
// Text formatting
// ============================================================================

#[derive(Debug, Clone)]
pub struct FormatOptions {
    pub show_ids: bool,
    pub show_urls: bool,
    pub show_dates: bool,
    pub show_assignee: bool,
    pub show_state: bool,
    pub show_team: bool,
    pub show_priority: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            show_ids: false,
            show_urls: false,
            show_dates: false,
            show_assignee: true,
            show_state: true,
            show_team: false,
            show_priority: true,
        }
    }
}

impl FormatOptions {
    pub fn from_env() -> Self {
        Self::from_csv(&std::env::var("LINEAR_TOOLS_EXTRAS").unwrap_or_default())
    }

    pub fn from_csv(csv: &str) -> Self {
        let mut o = Self::default();
        for f in csv
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
        {
            match f.as_str() {
                "id" | "ids" => o.show_ids = true,
                "url" | "urls" => o.show_urls = true,
                "date" | "dates" => o.show_dates = true,
                "assignee" | "assignees" => o.show_assignee = true,
                "state" | "states" => o.show_state = true,
                "team" | "teams" => o.show_team = true,
                "priority" | "priorities" => o.show_priority = true,
                _ => {}
            }
        }
        o
    }
}

impl TextFormat for SearchResult {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        if self.issues.is_empty() {
            return "Issues: <none>".into();
        }
        let o = FormatOptions::from_env();
        let mut out = String::new();
        let _ = writeln!(out, "Issues:");
        for i in &self.issues {
            let mut line = format!("{} - {}", i.identifier, i.title);
            if o.show_state
                && let Some(s) = &i.state
            {
                line.push_str(&format!(" [{}]", s.name));
            }
            if o.show_assignee
                && let Some(u) = &i.assignee
            {
                line.push_str(&format!(" (by {})", u.name));
            }
            if o.show_priority {
                line.push_str(&format!(" P{} ({})", i.priority, i.priority_label));
            }
            if o.show_team {
                line.push_str(&format!(" [{}]", i.team.key));
            }
            if o.show_urls {
                line.push_str(&format!(" {}", i.url));
            }
            if o.show_ids {
                line.push_str(&format!(" #{}", i.id));
            }
            if o.show_dates {
                line.push_str(&format!(" @{}", i.updated_at));
            }
            let _ = writeln!(out, "  {}", line);
        }
        if self.has_next_page && self.end_cursor.is_some() {
            let _ = writeln!(
                out,
                "\n[More results available, cursor: {}]",
                self.end_cursor.as_ref().unwrap()
            );
        }
        out
    }
}

impl TextFormat for IssueDetails {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let o = FormatOptions::from_env();
        let i = &self.issue;
        let mut out = String::new();

        // Header line
        let _ = writeln!(out, "{}: {}", i.identifier, i.title);

        // Metadata line
        let mut meta = Vec::new();
        if let Some(s) = &i.state {
            meta.push(format!("Status: {}", s.name));
        }
        if o.show_priority {
            meta.push(format!("Priority: P{} ({})", i.priority, i.priority_label));
        }
        if o.show_assignee
            && let Some(u) = &i.assignee
        {
            meta.push(format!("Assignee: {}", u.name));
        }
        if o.show_team {
            meta.push(format!("Team: {}", i.team.key));
        }
        if let Some(p) = &i.project {
            meta.push(format!("Project: {}", p.name));
        }
        if !meta.is_empty() {
            let _ = writeln!(out, "{}", meta.join(" | "));
        }

        if o.show_urls {
            let _ = writeln!(out, "URL: {}", i.url);
        }
        if o.show_dates {
            let _ = writeln!(out, "Created: {} | Updated: {}", i.created_at, i.updated_at);
        }

        // Description
        if self
            .description
            .as_ref()
            .is_some_and(|d| !d.trim().is_empty())
        {
            let _ = writeln!(out, "\n{}", self.description.as_ref().unwrap());
        }

        out
    }
}

impl TextFormat for CreateIssueResult {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        if !self.success {
            return "Failed to create issue".into();
        }
        match &self.issue {
            Some(i) => format!("Created issue: {} - {}", i.identifier, i.title),
            None => "Issue created (no details returned)".into(),
        }
    }
}

impl TextFormat for CommentResult {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        if !self.success {
            return "Failed to add comment".into();
        }
        match (&self.comment_id, &self.body) {
            (Some(id), Some(body)) => {
                // 80 total, reserve 3 for "..."
                let preview = if body.chars().count() > 80 {
                    format!("{}...", truncate_chars(body, 77))
                } else {
                    body.clone()
                };
                format!("Comment added ({}): {}", id, preview)
            }
            _ => "Comment added".into(),
        }
    }
}

// ============================================================================
// Archive + Metadata models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArchiveIssueResult {
    pub success: bool,
}

impl TextFormat for ArchiveIssueResult {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        if self.success {
            "Issue archived successfully".into()
        } else {
            "Failed to archive issue".into()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetadataKind {
    Users,
    Teams,
    Projects,
    WorkflowStates,
    Labels,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MetadataItem {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetMetadataResult {
    pub kind: MetadataKind,
    pub items: Vec<MetadataItem>,
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

impl TextFormat for GetMetadataResult {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        if self.items.is_empty() {
            return format!("{:?}: <none>", self.kind);
        }
        let mut out = String::new();
        for item in &self.items {
            let mut line = format!("{} ({})", item.name, item.id);
            if let Some(ref key) = item.key {
                line = format!("{} [{}] ({})", item.name, key, item.id);
            }
            if let Some(ref email) = item.email {
                line.push_str(&format!(" <{}>", email));
            }
            if let Some(ref st) = item.state_type {
                line.push_str(&format!(" [{}]", st));
            }
            let _ = writeln!(out, "  {}", line);
        }
        if self.has_next_page
            && let Some(ref cursor) = self.end_cursor
        {
            let _ = writeln!(out, "  (more results: after={})", cursor);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_ascii_safely() {
        let s = "abcdefghijklmnopqrstuvwxyz";
        assert_eq!(truncate_chars(s, 5), "abcde");
    }

    #[test]
    fn truncates_utf8_safely() {
        let s = "hello 😀😃😄😁"; // multi-byte
        let truncated = truncate_chars(s, 8);
        assert_eq!(truncated.chars().count(), 8);
        assert_eq!(truncated, "hello 😀😃");
    }

    #[test]
    fn handles_short_strings() {
        assert_eq!(truncate_chars("hi", 10), "hi");
    }

    #[test]
    fn format_options_default_shows_state_assignee_priority() {
        let opts = FormatOptions::default();
        assert!(opts.show_state);
        assert!(opts.show_assignee);
        assert!(opts.show_priority);
        assert!(!opts.show_ids);
        assert!(!opts.show_urls);
        assert!(!opts.show_dates);
        assert!(!opts.show_team);
    }

    #[test]
    fn format_options_csv_adds_to_defaults() {
        let opts = FormatOptions::from_csv("id,url");
        assert!(opts.show_ids);
        assert!(opts.show_urls);
        // defaults still true
        assert!(opts.show_state);
        assert!(opts.show_assignee);
        assert!(opts.show_priority);
    }
}
