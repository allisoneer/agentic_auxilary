//! Tool wrappers for linear_tools using agentic-tools-core.
//!
//! Each tool delegates to the corresponding method on [`LinearTools`].

use crate::LinearTools;
use crate::models::{CommentResult, CreateIssueResult, IssueDetails, SearchResult};
use agentic_tools_core::{Tool, ToolContext, ToolError, ToolRegistry};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

// ============================================================================
// SearchIssues Tool
// ============================================================================

/// Input for search_issues tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchIssuesInput {
    /// Full-text search term (searches title, description, and optionally comments)
    #[serde(default)]
    pub query: Option<String>,
    /// Include comments in full-text search (default: true, only applies when query is provided)
    #[serde(default)]
    pub include_comments: Option<bool>,
    /// Filter by priority (0=None, 1=Urgent, 2=High, 3=Normal, 4=Low)
    #[serde(default)]
    pub priority: Option<i32>,
    /// Workflow state ID (UUID)
    #[serde(default)]
    pub state_id: Option<String>,
    /// Assignee user ID (UUID)
    #[serde(default)]
    pub assignee_id: Option<String>,
    /// Team ID (UUID)
    #[serde(default)]
    pub team_id: Option<String>,
    /// Project ID (UUID)
    #[serde(default)]
    pub project_id: Option<String>,
    /// Only issues created after this ISO 8601 date
    #[serde(default)]
    pub created_after: Option<String>,
    /// Only issues created before this ISO 8601 date
    #[serde(default)]
    pub created_before: Option<String>,
    /// Only issues updated after this ISO 8601 date
    #[serde(default)]
    pub updated_after: Option<String>,
    /// Only issues updated before this ISO 8601 date
    #[serde(default)]
    pub updated_before: Option<String>,
    /// Page size (default 50, max 100)
    #[serde(default)]
    pub first: Option<i32>,
    /// Pagination cursor
    #[serde(default)]
    pub after: Option<String>,
}

/// Tool for searching Linear issues.
#[derive(Clone)]
pub struct SearchIssuesTool {
    linear: Arc<LinearTools>,
}

impl SearchIssuesTool {
    pub fn new(linear: Arc<LinearTools>) -> Self {
        Self { linear }
    }
}

impl Tool for SearchIssuesTool {
    type Input = SearchIssuesInput;
    type Output = SearchResult;
    const NAME: &'static str = "search_issues";
    const DESCRIPTION: &'static str = "Search Linear issues using full-text search and/or filters";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let linear = self.linear.clone();
        Box::pin(async move {
            linear
                .search_issues(
                    input.query,
                    input.include_comments,
                    input.priority,
                    input.state_id,
                    input.assignee_id,
                    input.team_id,
                    input.project_id,
                    input.created_after,
                    input.created_before,
                    input.updated_after,
                    input.updated_before,
                    input.first,
                    input.after,
                )
                .await
                .map_err(map_anyhow_to_tool_error)
        })
    }
}

// ============================================================================
// ReadIssue Tool
// ============================================================================

/// Input for read_issue tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadIssueInput {
    /// Issue ID, identifier (e.g., ENG-245), or URL
    pub issue: String,
}

/// Tool for reading a single Linear issue.
#[derive(Clone)]
pub struct ReadIssueTool {
    linear: Arc<LinearTools>,
}

impl ReadIssueTool {
    pub fn new(linear: Arc<LinearTools>) -> Self {
        Self { linear }
    }
}

impl Tool for ReadIssueTool {
    type Input = ReadIssueInput;
    type Output = IssueDetails;
    const NAME: &'static str = "read_issue";
    const DESCRIPTION: &'static str =
        "Read a Linear issue by ID, identifier (e.g., ENG-245), or URL";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let linear = self.linear.clone();
        Box::pin(async move {
            linear
                .read_issue(input.issue)
                .await
                .map_err(map_anyhow_to_tool_error)
        })
    }
}

// ============================================================================
// CreateIssue Tool
// ============================================================================

/// Input for create_issue tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CreateIssueInput {
    /// Team ID (UUID) to create the issue in
    pub team_id: String,
    /// Issue title
    pub title: String,
    /// Issue description (markdown supported)
    #[serde(default)]
    pub description: Option<String>,
    /// Priority (0=None, 1=Urgent, 2=High, 3=Normal, 4=Low)
    #[serde(default)]
    pub priority: Option<i32>,
    /// Assignee user ID (UUID)
    #[serde(default)]
    pub assignee_id: Option<String>,
    /// Project ID (UUID)
    #[serde(default)]
    pub project_id: Option<String>,
    /// Workflow state ID (UUID)
    #[serde(default)]
    pub state_id: Option<String>,
    /// Parent issue ID (UUID) for sub-issues
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Label IDs (UUIDs)
    #[serde(default)]
    pub label_ids: Vec<String>,
}

/// Tool for creating a new Linear issue.
#[derive(Clone)]
pub struct CreateIssueTool {
    linear: Arc<LinearTools>,
}

impl CreateIssueTool {
    pub fn new(linear: Arc<LinearTools>) -> Self {
        Self { linear }
    }
}

impl Tool for CreateIssueTool {
    type Input = CreateIssueInput;
    type Output = CreateIssueResult;
    const NAME: &'static str = "create_issue";
    const DESCRIPTION: &'static str = "Create a new Linear issue in a team";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let linear = self.linear.clone();
        Box::pin(async move {
            linear
                .create_issue(
                    input.team_id,
                    input.title,
                    input.description,
                    input.priority,
                    input.assignee_id,
                    input.project_id,
                    input.state_id,
                    input.parent_id,
                    input.label_ids,
                )
                .await
                .map_err(map_anyhow_to_tool_error)
        })
    }
}

// ============================================================================
// AddComment Tool
// ============================================================================

/// Input for add_comment tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AddCommentInput {
    /// Issue ID, identifier (e.g., ENG-245), or URL
    pub issue: String,
    /// Comment body (markdown supported)
    pub body: String,
    /// Parent comment ID for replies (UUID)
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Tool for adding a comment to a Linear issue.
#[derive(Clone)]
pub struct AddCommentTool {
    linear: Arc<LinearTools>,
}

impl AddCommentTool {
    pub fn new(linear: Arc<LinearTools>) -> Self {
        Self { linear }
    }
}

impl Tool for AddCommentTool {
    type Input = AddCommentInput;
    type Output = CommentResult;
    const NAME: &'static str = "add_comment";
    const DESCRIPTION: &'static str = "Add a comment to a Linear issue";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let linear = self.linear.clone();
        Box::pin(async move {
            linear
                .add_comment(input.issue, input.body, input.parent_id)
                .await
                .map_err(map_anyhow_to_tool_error)
        })
    }
}

// ============================================================================
// Registry Builder
// ============================================================================

/// Build a ToolRegistry containing all linear_tools tools.
pub fn build_registry(linear: Arc<LinearTools>) -> ToolRegistry {
    ToolRegistry::builder()
        .register::<SearchIssuesTool, ()>(SearchIssuesTool::new(linear.clone()))
        .register::<ReadIssueTool, ()>(ReadIssueTool::new(linear.clone()))
        .register::<CreateIssueTool, ()>(CreateIssueTool::new(linear.clone()))
        .register::<AddCommentTool, ()>(AddCommentTool::new(linear))
        .finish()
}

// ============================================================================
// Error Conversion
// ============================================================================

/// Map anyhow::Error to agentic_tools_core::ToolError based on error message patterns.
fn map_anyhow_to_tool_error(e: anyhow::Error) -> ToolError {
    let msg = e.to_string();
    let lc = msg.to_lowercase();
    if lc.contains("permission") || lc.contains("401") || lc.contains("403") {
        ToolError::Permission(msg)
    } else if lc.contains("not found") || lc.contains("404") {
        ToolError::NotFound(msg)
    } else if lc.contains("invalid") || lc.contains("bad request") {
        ToolError::InvalidInput(msg)
    } else if lc.contains("timeout") || lc.contains("network") || lc.contains("rate limit") {
        ToolError::External(msg)
    } else {
        ToolError::Internal(msg)
    }
}
