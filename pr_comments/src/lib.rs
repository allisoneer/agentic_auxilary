pub mod git;
pub mod github;
pub mod models;

use anyhow::{Context, Result};
use models::{AllComments, IssueCommentList, PrSummaryList, ReviewCommentList};
use std::sync::Arc;
use universal_tool_core::prelude::*;

#[derive(Clone)]
pub struct PrComments {
    owner: String,
    repo: String,
    token: Option<String>,
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

    pub fn new() -> Result<Self> {
        let git_info = git::get_git_info().context("Failed to get git information")?;
        let token = Self::get_token()?;

        Ok(Self {
            owner: git_info.owner,
            repo: git_info.repo,
            token,
        })
    }

    pub fn with_repo(owner: String, repo: String) -> Self {
        let token = Self::get_token().ok().flatten();
        Self { owner, repo, token }
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
    /// Get all comments (both review and issue comments) for a PR
    #[universal_tool(
        description = "Get all comments for a PR",
        cli(name = "all", alias = "get"),
        mcp(read_only = true, output = "text")
    )]
    pub async fn get_all_comments(
        &self,
        #[universal_tool_param(description = "PR number (auto-detected if not provided)")]
        pr_number: Option<u64>,
    ) -> Result<AllComments, ToolError> {
        let pr = self
            .get_pr_number(pr_number)
            .await
            .map_err(|e| ToolError::new(ErrorCode::InvalidArgument, e.to_string()))?;

        let client =
            github::GitHubClient::new(self.owner.clone(), self.repo.clone(), self.token.clone())
                .map_err(|e| ToolError::new(ErrorCode::Internal, e.to_string()))?;

        client.get_all_comments(pr).await
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
            })
    }

    /// Get only review comments (code comments) for a PR
    #[universal_tool(
        description = "Get review comments (code comments) for a PR",
        cli(name = "review-comments", alias = "review"),
        mcp(read_only = true, output = "text")
    )]
    pub async fn get_review_comments(
        &self,
        #[universal_tool_param(description = "PR number (auto-detected if not provided)")]
        pr_number: Option<u64>,
        #[universal_tool_param(
            description = "Include resolved review comments (defaults to false)"
        )]
        include_resolved: Option<bool>,
        #[universal_tool_param(
            description = "Include replies to top-level comments (defaults to true)"
        )]
        include_replies: Option<bool>,
        #[universal_tool_param(
            description = "Filter by top-level author login; includes all replies under matching parents"
        )]
        author: Option<String>,
        #[universal_tool_param(
            description = "Number of filtered comments to skip before returning results"
        )]
        offset: Option<usize>,
        #[universal_tool_param(description = "Limit the number of filtered comments returned")]
        limit: Option<usize>,
    ) -> Result<ReviewCommentList, ToolError> {
        let pr = self
            .get_pr_number(pr_number)
            .await
            .map_err(|e| ToolError::new(ErrorCode::InvalidArgument, e.to_string()))?;

        let client =
            github::GitHubClient::new(self.owner.clone(), self.repo.clone(), self.token.clone())
                .map_err(|e| ToolError::new(ErrorCode::Internal, e.to_string()))?;

        let comments = client
            .get_review_comments(
                pr,
                include_resolved,
                include_replies,
                author.as_deref(),
                offset,
                limit,
            )
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("401") || msg.contains("403") {
                    ToolError::new(
                        ErrorCode::PermissionDenied,
                        format!(
                            "{}\n\nHint: For private repositories, ensure your token has the 'repo' scope.",
                            msg
                        )
                    )
                } else {
                    ToolError::new(ErrorCode::ExternalServiceError, msg)
                }
            })?;

        Ok(ReviewCommentList { comments })
    }

    /// Get only issue comments (discussion comments) for a PR
    #[universal_tool(
        description = "Get issue comments (discussion) for a PR",
        cli(name = "issue-comments", alias = "discussion"),
        mcp(read_only = true, output = "text")
    )]
    pub async fn get_issue_comments(
        &self,
        #[universal_tool_param(description = "PR number (auto-detected if not provided)")]
        pr_number: Option<u64>,
        #[universal_tool_param(description = "Filter by author login")] author: Option<String>,
        #[universal_tool_param(
            description = "Number of filtered comments to skip before returning results"
        )]
        offset: Option<usize>,
        #[universal_tool_param(description = "Limit the number of filtered comments returned")]
        limit: Option<usize>,
    ) -> Result<IssueCommentList, ToolError> {
        let pr = self
            .get_pr_number(pr_number)
            .await
            .map_err(|e| ToolError::new(ErrorCode::InvalidArgument, e.to_string()))?;

        let client =
            github::GitHubClient::new(self.owner.clone(), self.repo.clone(), self.token.clone())
                .map_err(|e| ToolError::new(ErrorCode::Internal, e.to_string()))?;

        let comments = client
            .get_issue_comments(pr, author.as_deref(), offset, limit)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("401") || msg.contains("403") {
                    ToolError::new(
                        ErrorCode::PermissionDenied,
                        format!(
                            "{}\n\nHint: For private repositories, ensure your token has the 'repo' scope.",
                            msg
                        )
                    )
                } else {
                    ToolError::new(ErrorCode::ExternalServiceError, msg)
                }
            })?;
        Ok(IssueCommentList { comments })
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
