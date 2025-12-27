use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use universal_tool_core::mcp::McpFormatter;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueSummary {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub state: Option<String>,
    pub assignee: Option<String>,
    pub priority: Option<i32>,
    pub url: Option<String>,
    pub team_key: Option<String>,
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
    pub project: Option<String>,
    pub created_at: String,
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

#[derive(Debug, Clone, Default)]
pub struct FormatOptions {
    pub show_ids: bool,
    pub show_urls: bool,
    pub show_dates: bool,
    pub show_assignee: bool,
    pub show_state: bool,
    pub show_team: bool,
    pub show_priority: bool,
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

impl McpFormatter for SearchResult {
    fn mcp_format_text(&self) -> String {
        if self.issues.is_empty() {
            return "Issues: <none>".into();
        }
        let o = FormatOptions::from_env();
        let mut out = String::new();
        let _ = writeln!(out, "Issues:");
        for i in &self.issues {
            let mut line = format!("{} - {}", i.identifier, i.title);
            if o.show_state && i.state.is_some() {
                line.push_str(&format!(" [{}]", i.state.as_ref().unwrap()));
            }
            if o.show_assignee && i.assignee.is_some() {
                line.push_str(&format!(" (by {})", i.assignee.as_ref().unwrap()));
            }
            if o.show_priority && i.priority.is_some() {
                line.push_str(&format!(" P{}", i.priority.unwrap()));
            }
            if o.show_team && i.team_key.is_some() {
                line.push_str(&format!(" [{}]", i.team_key.as_ref().unwrap()));
            }
            if o.show_urls && i.url.is_some() {
                line.push_str(&format!(" {}", i.url.as_ref().unwrap()));
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

impl McpFormatter for IssueDetails {
    fn mcp_format_text(&self) -> String {
        let o = FormatOptions::from_env();
        let i = &self.issue;
        let mut out = String::new();

        // Header line
        let _ = writeln!(out, "{}: {}", i.identifier, i.title);

        // Metadata line
        let mut meta = Vec::new();
        if let Some(s) = &i.state {
            meta.push(format!("Status: {}", s));
        }
        if o.show_priority && i.priority.is_some() {
            meta.push(format!("Priority: P{}", i.priority.unwrap()));
        }
        if o.show_assignee && i.assignee.is_some() {
            meta.push(format!("Assignee: {}", i.assignee.as_ref().unwrap()));
        }
        if o.show_team && i.team_key.is_some() {
            meta.push(format!("Team: {}", i.team_key.as_ref().unwrap()));
        }
        if let Some(p) = &self.project {
            meta.push(format!("Project: {}", p));
        }
        if !meta.is_empty() {
            let _ = writeln!(out, "{}", meta.join(" | "));
        }

        if o.show_urls && i.url.is_some() {
            let _ = writeln!(out, "URL: {}", i.url.as_ref().unwrap());
        }
        if o.show_dates {
            let _ = writeln!(
                out,
                "Created: {} | Updated: {}",
                self.created_at, i.updated_at
            );
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

impl McpFormatter for CreateIssueResult {
    fn mcp_format_text(&self) -> String {
        if !self.success {
            return "Failed to create issue".into();
        }
        match &self.issue {
            Some(i) => format!("Created issue: {} - {}", i.identifier, i.title),
            None => "Issue created (no details returned)".into(),
        }
    }
}

impl McpFormatter for CommentResult {
    fn mcp_format_text(&self) -> String {
        if !self.success {
            return "Failed to add comment".into();
        }
        match (&self.comment_id, &self.body) {
            (Some(id), Some(body)) => {
                let preview = if body.len() > 80 {
                    format!("{}...", &body[..77])
                } else {
                    body.clone()
                };
                format!("Comment added ({}): {}", id, preview)
            }
            _ => "Comment added".into(),
        }
    }
}
