//! Tool wrappers for pr_comments using agentic-tools-core.
//!
//! Each tool delegates to the corresponding method on [`PrComments`].

use crate::PrComments;
use crate::models::{CommentSourceType, PrSummaryList, ReviewComment, ReviewCommentList};
use agentic_tools_core::{Tool, ToolContext, ToolError, ToolRegistry};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

// ============================================================================
// GetComments Tool
// ============================================================================

/// Input for get_comments tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GetCommentsInput {
    /// PR number (auto-detected if not provided)
    #[serde(default)]
    pub pr_number: Option<u64>,
    /// Filter by comment source: robot, human, or all
    #[serde(default)]
    pub comment_source_type: Option<CommentSourceType>,
    /// Include resolved review comments (defaults to false)
    #[serde(default)]
    pub include_resolved: Option<bool>,
}

/// Tool for fetching PR review comments with pagination.
#[derive(Clone)]
pub struct GetCommentsTool {
    pr_comments: Arc<PrComments>,
}

impl GetCommentsTool {
    pub fn new(pr_comments: Arc<PrComments>) -> Self {
        Self { pr_comments }
    }
}

impl Tool for GetCommentsTool {
    type Input = GetCommentsInput;
    type Output = ReviewCommentList;
    const NAME: &'static str = "gh_get_comments";
    const DESCRIPTION: &'static str = "Get PR review comments with thread-level pagination. Repeated calls with same params return next page.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let pr_comments = self.pr_comments.clone();
        Box::pin(async move {
            pr_comments
                .get_comments(
                    input.pr_number,
                    input.comment_source_type,
                    input.include_resolved,
                )
                .await
                .map_err(map_anyhow_to_tool_error)
        })
    }
}

// ============================================================================
// ListPrs Tool
// ============================================================================

/// Input for list_prs tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListPrsInput {
    /// PR state filter: open, closed, or all
    #[serde(default)]
    pub state: Option<String>,
}

/// Tool for listing pull requests in the repository.
#[derive(Clone)]
pub struct ListPrsTool {
    pr_comments: Arc<PrComments>,
}

impl ListPrsTool {
    pub fn new(pr_comments: Arc<PrComments>) -> Self {
        Self { pr_comments }
    }
}

impl Tool for ListPrsTool {
    type Input = ListPrsInput;
    type Output = PrSummaryList;
    const NAME: &'static str = "gh_get_prs";
    const DESCRIPTION: &'static str = "List pull requests in the repository";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let pr_comments = self.pr_comments.clone();
        Box::pin(async move {
            pr_comments
                .list_prs(input.state)
                .await
                .map_err(map_anyhow_to_tool_error)
        })
    }
}

// ============================================================================
// AddCommentReply Tool
// ============================================================================

/// Input for add_comment_reply tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AddCommentReplyInput {
    /// PR number (auto-detected if not provided)
    #[serde(default)]
    pub pr_number: Option<u64>,
    /// ID of the comment to reply to
    pub comment_id: u64,
    /// Reply message body
    pub body: String,
}

/// Tool for replying to a PR review comment.
#[derive(Clone)]
pub struct AddCommentReplyTool {
    pr_comments: Arc<PrComments>,
}

impl AddCommentReplyTool {
    pub fn new(pr_comments: Arc<PrComments>) -> Self {
        Self { pr_comments }
    }
}

impl Tool for AddCommentReplyTool {
    type Input = AddCommentReplyInput;
    type Output = ReviewComment;
    const NAME: &'static str = "gh_add_comment_reply";
    const DESCRIPTION: &'static str = "Reply to a PR review comment. Automatically prefixes with AI identifier to clearly mark automated responses.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let pr_comments = self.pr_comments.clone();
        Box::pin(async move {
            pr_comments
                .add_comment_reply(input.pr_number, input.comment_id, input.body)
                .await
                .map_err(map_anyhow_to_tool_error)
        })
    }
}

// ============================================================================
// Registry Builder
// ============================================================================

/// Build a ToolRegistry containing all pr_comments tools.
pub fn build_registry(pr_comments: Arc<PrComments>) -> ToolRegistry {
    ToolRegistry::builder()
        .register::<GetCommentsTool, ()>(GetCommentsTool::new(pr_comments.clone()))
        .register::<ListPrsTool, ()>(ListPrsTool::new(pr_comments.clone()))
        .register::<AddCommentReplyTool, ()>(AddCommentReplyTool::new(pr_comments))
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
    } else if lc.contains("timeout") || lc.contains("network") {
        ToolError::External(msg)
    } else {
        ToolError::Internal(msg)
    }
}
