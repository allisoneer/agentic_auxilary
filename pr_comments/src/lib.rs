pub mod git;
pub mod github;
pub mod models;
pub mod pagination;

use anyhow::{Context, Result};
use models::{CommentSourceType, PrSummaryList, ReviewComment, ReviewCommentList, Thread};
use pagination::{PaginationCache, make_key, paginate_slice};
use std::sync::Arc;
use universal_tool_core::prelude::*;

/// AI response prefix to clearly identify automated replies.
pub const AI_PREFIX: &str = "\u{1F916} AI response: ";

/// Prepend the AI prefix to a message body if it's not already present.
/// Avoids double-prefixing, handles leading whitespace.
pub fn with_ai_prefix(body: &str) -> String {
    if body.trim_start().starts_with(AI_PREFIX) {
        body.to_string()
    } else {
        format!("{}{}", AI_PREFIX, body)
    }
}

#[derive(Clone)]
pub struct PrComments {
    owner: String,
    repo: String,
    token: Option<String>,
    pager: Arc<PaginationCache<Thread>>,
}

impl PrComments {
    fn get_token() -> Result<Option<String>> {
        // 1) Check environment variables first (explicit override)
        if let Ok(t) = std::env::var("GH_TOKEN").or_else(|_| std::env::var("GITHUB_TOKEN")) {
            tracing::debug!("Using GitHub token from environment");
            return Ok(Some(t));
        }

        // 2) Try gh-config (hosts.yml → keyring)
        match gh_config::Hosts::load() {
            Ok(hosts) => match hosts.retrieve_token(gh_config::GITHUB_COM) {
                Ok(Some(t)) => {
                    tracing::debug!("Using GitHub token from gh-config");
                    Ok(Some(t))
                }
                Ok(None) => {
                    tracing::debug!("No token found in gh-config");
                    Ok(None)
                }
                Err(e) => {
                    tracing::debug!("gh-config token retrieval failed: {}", e);
                    Ok(None)
                }
            },
            Err(e) => {
                tracing::debug!("gh-config unavailable: {}", e);
                Ok(None)
            }
        }
    }

    /// Get page size from environment variable PR_COMMENTS_PAGE_SIZE.
    /// Defaults to 10, clamped to [1, 1000].
    fn page_size_from_env() -> usize {
        std::env::var("PR_COMMENTS_PAGE_SIZE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n >= 1 && *n <= 1000)
            .unwrap_or(10)
    }

    pub fn new() -> Result<Self> {
        let git_info = git::get_git_info().context("Failed to get git information")?;
        let token = Self::get_token()?;

        Ok(Self {
            owner: git_info.owner,
            repo: git_info.repo,
            token,
            pager: Arc::new(PaginationCache::new()),
        })
    }

    pub fn with_repo(owner: String, repo: String) -> Self {
        let token = Self::get_token().ok().flatten();
        Self {
            owner,
            repo,
            token,
            pager: Arc::new(PaginationCache::new()),
        }
    }

    async fn get_pr_number(&self, pr_number: Option<u64>) -> Result<u64> {
        if let Some(pr) = pr_number {
            return Ok(pr);
        }

        // Try to detect from current branch
        let git_info = git::get_git_info()?;
        let branch = git_info
            .current_branch
            .context("Could not determine current git branch")?;

        let client =
            github::GitHubClient::new(self.owner.clone(), self.repo.clone(), self.token.clone())?;

        match client.get_pr_from_branch(&branch).await {
            Ok(Some(pr)) => Ok(pr),
            Ok(None) => Err(anyhow::anyhow!(
                "No open PR found for branch '{}' in {}/{}. \n\
                Make sure you have an open PR for this branch.",
                branch,
                self.owner,
                self.repo
            )),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("401") || msg.contains("403") || msg.contains("Not Found") {
                    Err(anyhow::anyhow!(
                        "Failed to access {}/{}: {}\n\n\
                        Hint: For private repositories, ensure your GITHUB_TOKEN has the 'repo' scope.\n\
                        Current token: {}",
                        self.owner,
                        self.repo,
                        msg,
                        if self.token.is_some() {
                            "Set"
                        } else {
                            "Not set (required for private repos)"
                        }
                    ))
                } else {
                    Err(e)
                }
            }
        }
    }
}

#[universal_tool_router(
    cli(name = "pr-comments", description = "Fetch GitHub PR comments"),
    mcp(name = "pr-comments", version = "0.1.0")
)]
impl PrComments {
    /// Get PR review comments with thread-level pagination.
    /// Repeated calls with same params return next page.
    #[universal_tool(
        description = "Get PR review comments with thread-level pagination. Repeated calls with same params return next page.",
        cli(name = "comments"),
        mcp(read_only = true, output = "text")
    )]
    pub async fn get_comments(
        &self,
        #[universal_tool_param(description = "PR number (auto-detected if not provided)")]
        pr_number: Option<u64>,
        #[universal_tool_param(description = "Filter by comment source: robot, human, or all")]
        comment_source_type: Option<CommentSourceType>,
        #[universal_tool_param(
            description = "Include resolved review comments (defaults to false)"
        )]
        include_resolved: Option<bool>,
    ) -> Result<ReviewCommentList, ToolError> {
        let pr = self
            .get_pr_number(pr_number)
            .await
            .map_err(|e| ToolError::new(ErrorCode::InvalidArgument, e.to_string()))?;

        let src = comment_source_type.unwrap_or_default();
        let include_resolved = include_resolved.unwrap_or(false);
        let page_size = Self::page_size_from_env();

        // Sweep expired cache entries opportunistically
        self.pager.sweep_expired();

        // Build cache key
        let key = make_key(
            &self.owner,
            &self.repo,
            pr,
            src,
            include_resolved,
            page_size,
        );

        // Get or create per-query lock
        let query_lock = self.pager.get_or_create(&key);

        // Check if we need to fetch data (quick check, release lock before await)
        let needs_fetch = {
            let state = query_lock.state.lock().unwrap();
            state.is_empty() || state.is_expired()
        };

        // If we need to fetch, do async work without holding the lock
        if needs_fetch {
            let client = github::GitHubClient::new(
                self.owner.clone(),
                self.repo.clone(),
                self.token.clone(),
            )
            .map_err(|e| ToolError::new(ErrorCode::Internal, e.to_string()))?;

            // Fetch all comments
            let comments = client.fetch_review_comments(pr).await.map_err(|e| {
                let msg = e.to_string();
                if msg.contains("401") || msg.contains("403") {
                    ToolError::new(
                        ErrorCode::PermissionDenied,
                        format!(
                            "{}\n\nHint: For private repositories, ensure your token has the 'repo' scope.",
                            msg
                        ),
                    )
                } else {
                    ToolError::new(ErrorCode::ExternalServiceError, msg)
                }
            })?;

            // Get resolution map
            let resolution_map = if !include_resolved {
                client
                    .get_review_thread_resolution_status(pr)
                    .await
                    .unwrap_or_default()
            } else {
                std::collections::HashMap::new()
            };

            // Build and filter threads
            let threads = client.build_threads(comments, &resolution_map);
            let filtered = client.filter_threads(threads, src, include_resolved);

            // Re-acquire lock to update state
            let mut state = query_lock.state.lock().unwrap();
            state.reset(filtered, page_size);
        }

        // Now paginate (re-acquire lock for pagination)
        let mut state = query_lock.state.lock().unwrap();
        let (page_threads, has_more) =
            paginate_slice(&state.results, state.next_offset, state.page_size);
        state.next_offset += page_threads.len();

        // Flatten threads to comments for output
        let comments: Vec<_> = page_threads
            .into_iter()
            .flat_map(|t| {
                let mut cs = vec![t.parent];
                cs.extend(t.replies);
                cs
            })
            .collect();

        // If no more pages, drop cache entry
        if !has_more {
            drop(state);
            self.pager.remove_if_same(&key, &query_lock);
        }

        Ok(ReviewCommentList { comments })
    }

    /// List pull requests in the repository
    #[universal_tool(
        description = "List pull requests in the repository",
        cli(name = "list-prs", alias = "list"),
        mcp(read_only = true, output = "text")
    )]
    pub async fn list_prs(
        &self,
        #[universal_tool_param(
            description = "PR state filter: open, closed, or all",
            default = "open"
        )]
        state: Option<String>,
    ) -> Result<PrSummaryList, ToolError> {
        let client =
            github::GitHubClient::new(self.owner.clone(), self.repo.clone(), self.token.clone())
                .map_err(|e| ToolError::new(ErrorCode::Internal, e.to_string()))?;

        let prs = client
            .list_prs(state)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("401") || msg.contains("403") {
                    ToolError::new(
                        ErrorCode::PermissionDenied,
                        format!("{}\n\nHint: For private repositories, ensure your GITHUB_TOKEN has the 'repo' scope.", msg)
                    )
                } else {
                    ToolError::new(ErrorCode::ExternalServiceError, msg)
                }
            })?;
        Ok(PrSummaryList { prs })
    }

    /// Reply to a PR review comment. Automatically prefixes with AI identifier.
    #[universal_tool(
        description = "Reply to a PR review comment. Automatically prefixes with AI identifier to clearly mark automated responses.",
        cli(name = "reply"),
        mcp(read_only = false, output = "text")
    )]
    pub async fn add_comment_reply(
        &self,
        #[universal_tool_param(description = "PR number (auto-detected if not provided)")]
        pr_number: Option<u64>,
        #[universal_tool_param(description = "ID of the comment to reply to")] comment_id: u64,
        #[universal_tool_param(description = "Reply message body")] body: String,
    ) -> Result<ReviewComment, ToolError> {
        // Validate body is not empty
        if body.trim().is_empty() {
            return Err(ToolError::new(
                ErrorCode::InvalidArgument,
                "Body cannot be empty".to_string(),
            ));
        }

        let pr = self
            .get_pr_number(pr_number)
            .await
            .map_err(|e| ToolError::new(ErrorCode::InvalidArgument, e.to_string()))?;

        let client =
            github::GitHubClient::new(self.owner.clone(), self.repo.clone(), self.token.clone())
                .map_err(|e| ToolError::new(ErrorCode::Internal, e.to_string()))?;

        // Apply AI prefix to clearly identify automated responses
        let prefixed_body = with_ai_prefix(&body);

        let comment = client
            .reply_to_comment(pr, comment_id, &prefixed_body)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("401") || msg.contains("403") {
                    ToolError::new(
                        ErrorCode::PermissionDenied,
                        format!(
                            "{}\n\nHint: For private repositories, ensure your token has the 'repo' scope.",
                            msg
                        ),
                    )
                } else if msg.contains("404") {
                    ToolError::new(
                        ErrorCode::NotFound,
                        format!("Comment {} not found on PR #{}", comment_id, pr),
                    )
                } else {
                    ToolError::new(ErrorCode::ExternalServiceError, msg)
                }
            })?;

        Ok(comment)
    }
}

// MCP server implementation
pub struct PrCommentsServer {
    tools: Arc<PrComments>,
}

impl PrCommentsServer {
    pub fn new(tools: Arc<PrComments>) -> Self {
        Self { tools }
    }
}

universal_tool_core::implement_mcp_server!(PrCommentsServer, tools);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_ai_prefix_adds_prefix() {
        let body = "This is a reply";
        let prefixed = with_ai_prefix(body);
        assert!(prefixed.starts_with(AI_PREFIX));
        assert_eq!(prefixed, format!("{}This is a reply", AI_PREFIX));
    }

    #[test]
    fn with_ai_prefix_avoids_double_prefix() {
        let already_prefixed = format!("{}Already prefixed", AI_PREFIX);
        let result = with_ai_prefix(&already_prefixed);
        assert_eq!(result, already_prefixed);
        // Ensure no double prefix
        assert_eq!(result.matches(AI_PREFIX).count(), 1);
    }

    #[test]
    fn with_ai_prefix_handles_empty_body() {
        let body = "";
        let prefixed = with_ai_prefix(body);
        assert_eq!(prefixed, AI_PREFIX);
    }

    #[test]
    fn with_ai_prefix_handles_leading_whitespace() {
        // Body with leading whitespace before prefix should not double-prefix
        let body_with_space = format!("  {}Already prefixed", AI_PREFIX);
        let result = with_ai_prefix(&body_with_space);
        assert_eq!(result, body_with_space);
        // Ensure no double prefix
        assert_eq!(result.matches(AI_PREFIX).count(), 1);
    }

    #[test]
    fn ai_prefix_contains_robot_emoji() {
        // Verify the prefix contains the robot emoji for clear AI identification
        assert!(AI_PREFIX.contains('\u{1F916}')); // 🤖
    }
}
