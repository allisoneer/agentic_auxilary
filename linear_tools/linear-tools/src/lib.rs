pub mod http;
pub mod models;

/// Test support utilities (for use in tests)
#[doc(hidden)]
pub mod test_support;

use cynic::{MutationBuilder, QueryBuilder};
use http::LinearClient;
use linear_queries::*;
use regex::Regex;
use std::sync::Arc;
use universal_tool_core::prelude::*;

#[derive(Clone)]
pub struct LinearTools {
    api_key: Option<String>,
}

impl LinearTools {
    pub fn new() -> Self {
        Self {
            api_key: std::env::var("LINEAR_API_KEY").ok(),
        }
    }

    fn resolve_issue_id(&self, input: &str) -> IssueIdentifier {
        // URL pattern: .../issue/ENG-123...
        let url_re = Regex::new(r"/issue/([A-Z]{2,10}-\d+)").unwrap();
        if let Some(caps) = url_re.captures(input) {
            let ident = caps.get(1).unwrap().as_str().to_string();
            return IssueIdentifier::Identifier(ident);
        }
        // Identifier pattern: ENG-123
        let ident_re = Regex::new(r"^[A-Z]{2,10}-\d+$").unwrap();
        if ident_re.is_match(input) {
            return IssueIdentifier::Identifier(input.to_string());
        }
        // Fallback: treat as ID/UUID
        IssueIdentifier::Id(input.to_string())
    }
}

impl Default for LinearTools {
    fn default() -> Self {
        Self::new()
    }
}

enum IssueIdentifier {
    Id(String),
    Identifier(String),
}

fn to_tool_error(e: anyhow::Error) -> ToolError {
    let msg = e.to_string();
    if msg.contains("401") || msg.contains("403") || msg.contains("LINEAR_API_KEY") {
        ToolError::new(
            ErrorCode::PermissionDenied,
            format!("{}\n\nHint: Ensure LINEAR_API_KEY is set and valid.", msg),
        )
    } else if msg.contains("429") {
        ToolError::new(
            ErrorCode::ExternalServiceError,
            format!("Rate limited: {}", msg),
        )
    } else if msg.contains("404") {
        ToolError::new(ErrorCode::NotFound, msg)
    } else {
        ToolError::new(ErrorCode::ExternalServiceError, msg)
    }
}

#[universal_tool_router(
    cli(name = "linear-tools", description = "Linear issue management tools"),
    mcp(name = "linear-tools", version = "0.1.0")
)]
impl LinearTools {
    /// Search Linear issues with filters
    #[universal_tool(
        description = "Search Linear issues using filters",
        cli(name = "search", alias = "s"),
        mcp(read_only = true, output = "text")
    )]
    pub async fn search_issues(
        &self,
        #[universal_tool_param(description = "Text to search in title")] query: Option<String>,
        #[universal_tool_param(
            description = "Filter by priority (1=Urgent, 2=High, 3=Normal, 4=Low)"
        )]
        priority: Option<i32>,
        #[universal_tool_param(description = "Page size (default 50, max 100)")] first: Option<i32>,
        #[universal_tool_param(description = "Pagination cursor")] after: Option<String>,
    ) -> Result<models::SearchResult, ToolError> {
        let client = LinearClient::new(self.api_key.clone()).map_err(to_tool_error)?;

        let mut filter = IssueFilter::default();
        if let Some(q) = query {
            filter.title = Some(StringComparator {
                contains_ignore_case: Some(q),
                ..Default::default()
            });
        }
        if let Some(p) = priority {
            filter.priority = Some(NullableNumberComparator {
                eq: Some(p as f64),
                ..Default::default()
            });
        }

        let op = IssuesQuery::build(IssuesArguments {
            first: Some(first.unwrap_or(50).clamp(1, 100)),
            after,
            filter: Some(filter),
        });

        let resp = client.run(op).await.map_err(to_tool_error)?;
        let data = resp.data.ok_or_else(|| {
            ToolError::new(
                ErrorCode::ExternalServiceError,
                "No data returned from Linear",
            )
        })?;

        let issues = data
            .issues
            .nodes
            .into_iter()
            .map(|i| models::IssueSummary {
                id: i.id.inner().to_string(),
                identifier: i.identifier,
                title: i.title,
                state: i.state.map(|s| s.name),
                assignee: i.assignee.map(|u| {
                    if u.display_name.is_empty() {
                        u.name
                    } else {
                        u.display_name
                    }
                }),
                priority: Some(i.priority as i32),
                url: Some(i.url),
                team_key: Some(i.team.key),
                updated_at: i.updated_at.0,
            })
            .collect();

        Ok(models::SearchResult {
            issues,
            has_next_page: data.issues.page_info.has_next_page,
            end_cursor: data.issues.page_info.end_cursor,
        })
    }

    /// Read a single Linear issue
    #[universal_tool(
        description = "Read a Linear issue by ID, identifier (e.g., ENG-245), or URL",
        cli(name = "read", alias = "r"),
        mcp(read_only = true, output = "text")
    )]
    pub async fn read_issue(
        &self,
        #[universal_tool_param(description = "Issue ID, identifier (e.g., ENG-245), or URL")]
        issue: String,
    ) -> Result<models::IssueDetails, ToolError> {
        let client = LinearClient::new(self.api_key.clone()).map_err(to_tool_error)?;
        let resolved = self.resolve_issue_id(&issue);

        let issue_data = match resolved {
            IssueIdentifier::Id(id) => {
                let op = IssueByIdQuery::build(IssueByIdArguments { id });
                let resp = client.run(op).await.map_err(to_tool_error)?;
                let data = resp.data.ok_or_else(|| {
                    ToolError::new(ErrorCode::ExternalServiceError, "No data returned")
                })?;
                data.issue
                    .ok_or_else(|| ToolError::new(ErrorCode::NotFound, "Issue not found"))?
            }
            IssueIdentifier::Identifier(ident) => {
                // Search by title containing the identifier
                let filter = IssueFilter {
                    title: Some(StringComparator {
                        contains: Some(ident.clone()),
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                let op = IssuesQuery::build(IssuesArguments {
                    first: Some(50),
                    after: None,
                    filter: Some(filter),
                });
                let resp = client.run(op).await.map_err(to_tool_error)?;
                let data = resp.data.ok_or_else(|| {
                    ToolError::new(ErrorCode::ExternalServiceError, "No data returned")
                })?;
                // Find the issue with matching identifier
                data.issues
                    .nodes
                    .into_iter()
                    .find(|i| i.identifier == ident)
                    .ok_or_else(|| {
                        ToolError::new(ErrorCode::NotFound, format!("Issue {} not found", ident))
                    })?
            }
        };

        let summary = models::IssueSummary {
            id: issue_data.id.inner().to_string(),
            identifier: issue_data.identifier.clone(),
            title: issue_data.title.clone(),
            state: issue_data.state.map(|s| s.name),
            assignee: issue_data.assignee.map(|u| {
                if u.display_name.is_empty() {
                    u.name
                } else {
                    u.display_name
                }
            }),
            priority: Some(issue_data.priority as i32),
            url: Some(issue_data.url.clone()),
            team_key: Some(issue_data.team.key),
            updated_at: issue_data.updated_at.0.clone(),
        };

        Ok(models::IssueDetails {
            issue: summary,
            description: issue_data.description,
            project: issue_data.project.map(|p| p.name),
            created_at: issue_data.created_at.0,
        })
    }

    /// Create a new Linear issue
    #[universal_tool(
        description = "Create a new Linear issue in a team",
        cli(name = "create", alias = "c"),
        mcp(read_only = false, output = "text")
    )]
    pub async fn create_issue(
        &self,
        #[universal_tool_param(description = "Team ID (UUID) to create the issue in")]
        team_id: String,
        #[universal_tool_param(description = "Issue title")] title: String,
        #[universal_tool_param(description = "Issue description (markdown supported)")]
        description: Option<String>,
        #[universal_tool_param(
            description = "Priority (0=None, 1=Urgent, 2=High, 3=Normal, 4=Low)"
        )]
        priority: Option<i32>,
        #[universal_tool_param(description = "Assignee user ID (UUID)")] assignee_id: Option<
            String,
        >,
        #[universal_tool_param(description = "Project ID (UUID)")] project_id: Option<String>,
    ) -> Result<models::CreateIssueResult, ToolError> {
        let client = LinearClient::new(self.api_key.clone()).map_err(to_tool_error)?;

        let input = IssueCreateInput {
            team_id,
            title: Some(title),
            description,
            priority,
            assignee_id,
            project_id,
            ..Default::default()
        };

        let op = IssueCreateMutation::build(IssueCreateArguments { input });
        let resp = client.run(op).await.map_err(to_tool_error)?;
        let data = resp.data.ok_or_else(|| {
            ToolError::new(
                ErrorCode::ExternalServiceError,
                "No data returned from Linear",
            )
        })?;

        let payload = data.issue_create;
        let issue = payload.issue.map(|i| models::IssueSummary {
            id: i.id.inner().to_string(),
            identifier: i.identifier,
            title: i.title,
            state: i.state.map(|s| s.name),
            assignee: i.assignee.map(|u| {
                if u.display_name.is_empty() {
                    u.name
                } else {
                    u.display_name
                }
            }),
            priority: Some(i.priority as i32),
            url: Some(i.url),
            team_key: Some(i.team.key),
            updated_at: i.updated_at.0,
        });

        Ok(models::CreateIssueResult {
            success: payload.success,
            issue,
        })
    }

    /// Add a comment to a Linear issue
    #[universal_tool(
        description = "Add a comment to a Linear issue",
        cli(name = "comment", alias = "cm"),
        mcp(read_only = false, output = "text")
    )]
    pub async fn add_comment(
        &self,
        #[universal_tool_param(description = "Issue ID (UUID) to comment on")] issue_id: String,
        #[universal_tool_param(description = "Comment body (markdown supported)")] body: String,
        #[universal_tool_param(description = "Parent comment ID for replies (UUID)")]
        parent_id: Option<String>,
    ) -> Result<models::CommentResult, ToolError> {
        let client = LinearClient::new(self.api_key.clone()).map_err(to_tool_error)?;

        let input = CommentCreateInput {
            issue_id,
            body: Some(body),
            parent_id,
        };

        let op = CommentCreateMutation::build(CommentCreateArguments { input });
        let resp = client.run(op).await.map_err(to_tool_error)?;
        let data = resp.data.ok_or_else(|| {
            ToolError::new(
                ErrorCode::ExternalServiceError,
                "No data returned from Linear",
            )
        })?;

        let payload = data.comment_create;
        let (comment_id, body, created_at) = match payload.comment {
            Some(c) => (
                Some(c.id.inner().to_string()),
                Some(c.body),
                Some(c.created_at.0),
            ),
            None => (None, None, None),
        };

        Ok(models::CommentResult {
            success: payload.success,
            comment_id,
            body,
            created_at,
        })
    }
}

// MCP Server wrapper
pub struct LinearToolsServer {
    tools: Arc<LinearTools>,
}

impl LinearToolsServer {
    pub fn new(tools: Arc<LinearTools>) -> Self {
        Self { tools }
    }
}

universal_tool_core::implement_mcp_server!(LinearToolsServer, tools);
