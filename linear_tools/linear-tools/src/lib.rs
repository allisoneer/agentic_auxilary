pub mod http;
pub mod models;

/// Test support utilities (for use in tests)
#[doc(hidden)]
pub mod test_support;

use cynic::{MutationBuilder, QueryBuilder};
use http::LinearClient;
use linear_queries::scalars::DateTimeOrDuration;
use linear_queries::*;
use regex::Regex;
use std::sync::Arc;
use universal_tool_core::prelude::*;

/// Parse identifier "ENG-245" from plain text or URL; normalize to uppercase.
/// Returns (team_key, number) if a valid identifier is found.
fn parse_identifier(input: &str) -> Option<(String, i32)> {
    let upper = input.to_uppercase();
    let re = Regex::new(r"([A-Z]{2,10})-(\d{1,10})").unwrap();
    if let Some(caps) = re.captures(&upper) {
        let key = caps.get(1)?.as_str().to_string();
        let num_str = caps.get(2)?.as_str();
        let number: i32 = num_str.parse().ok()?;
        return Some((key, number));
    }
    None
}

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
        // Try to parse as identifier (handles lowercase and URLs)
        if let Some((key, number)) = parse_identifier(input) {
            return IssueIdentifier::Identifier(format!("{}-{}", key, number));
        }
        // Fallback: treat as ID/UUID
        IssueIdentifier::Id(input.to_string())
    }

    /// Resolve an issue identifier (UUID, ENG-245, or URL) to a UUID.
    /// For identifiers, queries Linear to find the matching issue.
    async fn resolve_to_issue_id(
        &self,
        client: &LinearClient,
        input: &str,
    ) -> Result<String, ToolError> {
        match self.resolve_issue_id(input) {
            IssueIdentifier::Id(id) => Ok(id),
            IssueIdentifier::Identifier(ident) => {
                let (team_key, number) = parse_identifier(&ident).ok_or_else(|| {
                    ToolError::new(ErrorCode::NotFound, format!("Issue {} not found", ident))
                })?;
                let filter = IssueFilter {
                    team: Some(TeamFilter {
                        key: Some(StringComparator {
                            eq: Some(team_key),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    number: Some(NumberComparator {
                        eq: Some(number as f64),
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                let op = IssuesQuery::build(IssuesArguments {
                    first: Some(1),
                    after: None,
                    filter: Some(filter),
                });
                let resp = client.run(op).await.map_err(to_tool_error)?;
                let data = http::extract_data(resp).map_err(to_tool_error)?;
                let issue = data.issues.nodes.into_iter().next().ok_or_else(|| {
                    ToolError::new(ErrorCode::NotFound, format!("Issue {} not found", ident))
                })?;
                Ok(issue.id.inner().to_string())
            }
        }
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
    /// Search Linear issues with full-text search or filters
    #[universal_tool(
        description = "Search Linear issues using full-text search and/or filters",
        cli(name = "search", alias = "s"),
        mcp(read_only = true, output = "text")
    )]
    #[allow(clippy::too_many_arguments)]
    pub async fn search_issues(
        &self,
        #[universal_tool_param(
            description = "Full-text search term (searches title, description, and optionally comments)"
        )]
        query: Option<String>,
        #[universal_tool_param(
            description = "Include comments in full-text search (default: true, only applies when query is provided)"
        )]
        include_comments: Option<bool>,
        #[universal_tool_param(
            description = "Filter by priority (1=Urgent, 2=High, 3=Normal, 4=Low)"
        )]
        priority: Option<i32>,
        #[universal_tool_param(description = "Workflow state ID (UUID)")] state_id: Option<String>,
        #[universal_tool_param(description = "Assignee user ID (UUID)")] assignee_id: Option<
            String,
        >,
        #[universal_tool_param(description = "Team ID (UUID)")] team_id: Option<String>,
        #[universal_tool_param(description = "Project ID (UUID)")] project_id: Option<String>,
        #[universal_tool_param(description = "Only issues created after this ISO 8601 date")]
        created_after: Option<String>,
        #[universal_tool_param(description = "Only issues created before this ISO 8601 date")]
        created_before: Option<String>,
        #[universal_tool_param(description = "Only issues updated after this ISO 8601 date")]
        updated_after: Option<String>,
        #[universal_tool_param(description = "Only issues updated before this ISO 8601 date")]
        updated_before: Option<String>,
        #[universal_tool_param(description = "Page size (default 50, max 100)")] first: Option<i32>,
        #[universal_tool_param(description = "Pagination cursor")] after: Option<String>,
    ) -> Result<models::SearchResult, ToolError> {
        let client = LinearClient::new(self.api_key.clone()).map_err(to_tool_error)?;

        // Build filters (no title filter - full-text search handles query)
        let mut filter = IssueFilter::default();
        let mut has_filter = false;

        if let Some(p) = priority {
            filter.priority = Some(NullableNumberComparator {
                eq: Some(p as f64),
                ..Default::default()
            });
            has_filter = true;
        }
        if let Some(id) = state_id {
            filter.state = Some(WorkflowStateFilter {
                id: Some(IdComparator {
                    eq: Some(cynic::Id::new(id)),
                }),
            });
            has_filter = true;
        }
        if let Some(id) = assignee_id {
            filter.assignee = Some(NullableUserFilter {
                id: Some(IdComparator {
                    eq: Some(cynic::Id::new(id)),
                }),
            });
            has_filter = true;
        }
        if let Some(id) = team_id {
            filter.team = Some(TeamFilter {
                id: Some(IdComparator {
                    eq: Some(cynic::Id::new(id)),
                }),
                ..Default::default()
            });
            has_filter = true;
        }
        if let Some(id) = project_id {
            filter.project = Some(NullableProjectFilter {
                id: Some(IdComparator {
                    eq: Some(cynic::Id::new(id)),
                }),
            });
            has_filter = true;
        }
        if created_after.is_some() || created_before.is_some() {
            filter.created_at = Some(DateComparator {
                gte: created_after.map(DateTimeOrDuration),
                lte: created_before.map(DateTimeOrDuration),
                ..Default::default()
            });
            has_filter = true;
        }
        if updated_after.is_some() || updated_before.is_some() {
            filter.updated_at = Some(DateComparator {
                gte: updated_after.map(DateTimeOrDuration),
                lte: updated_before.map(DateTimeOrDuration),
                ..Default::default()
            });
            has_filter = true;
        }

        let filter_opt = if has_filter { Some(filter) } else { None };
        let page_size = Some(first.unwrap_or(50).clamp(1, 100));
        let q_trimmed = query.as_ref().map(|s| s.trim()).unwrap_or("");

        if !q_trimmed.is_empty() {
            // Full-text search path: searchIssues
            let op = SearchIssuesQuery::build(SearchIssuesArguments {
                term: q_trimmed.to_string(),
                include_comments: Some(include_comments.unwrap_or(true)),
                first: page_size,
                after,
                filter: filter_opt,
            });
            let resp = client.run(op).await.map_err(to_tool_error)?;
            let data = http::extract_data(resp).map_err(to_tool_error)?;

            let issues = data
                .search_issues
                .nodes
                .into_iter()
                .map(|i| models::IssueSummary {
                    id: i.id.inner().to_string(),
                    identifier: i.identifier,
                    title: i.title,
                    state: Some(i.state.name),
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
                has_next_page: data.search_issues.page_info.has_next_page,
                end_cursor: data.search_issues.page_info.end_cursor,
            })
        } else {
            // Filters-only path: issues query
            let op = IssuesQuery::build(IssuesArguments {
                first: page_size,
                after,
                filter: filter_opt,
            });

            let resp = client.run(op).await.map_err(to_tool_error)?;
            let data = http::extract_data(resp).map_err(to_tool_error)?;

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
                let data = http::extract_data(resp).map_err(to_tool_error)?;
                data.issue
                    .ok_or_else(|| ToolError::new(ErrorCode::NotFound, "Issue not found"))?
            }
            IssueIdentifier::Identifier(ident) => {
                // Use server-side filtering by team.key + number
                let (team_key, number) = parse_identifier(&ident).ok_or_else(|| {
                    ToolError::new(ErrorCode::NotFound, format!("Issue {} not found", ident))
                })?;
                let filter = IssueFilter {
                    team: Some(TeamFilter {
                        key: Some(StringComparator {
                            eq: Some(team_key),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    number: Some(NumberComparator {
                        eq: Some(number as f64),
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                let op = IssuesQuery::build(IssuesArguments {
                    first: Some(1),
                    after: None,
                    filter: Some(filter),
                });
                let resp = client.run(op).await.map_err(to_tool_error)?;
                let data = http::extract_data(resp).map_err(to_tool_error)?;
                data.issues.nodes.into_iter().next().ok_or_else(|| {
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
    #[allow(clippy::too_many_arguments)]
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
        #[universal_tool_param(description = "Workflow state ID (UUID)")] state_id: Option<String>,
        #[universal_tool_param(description = "Parent issue ID (UUID) for sub-issues")]
        parent_id: Option<String>,
        #[universal_tool_param(description = "Label IDs (UUID). Pass multiple times to add many.")]
        label_ids: Vec<String>,
    ) -> Result<models::CreateIssueResult, ToolError> {
        let client = LinearClient::new(self.api_key.clone()).map_err(to_tool_error)?;

        // Convert empty Vec to None for the API
        let label_ids_opt = if label_ids.is_empty() {
            None
        } else {
            Some(label_ids)
        };

        let input = IssueCreateInput {
            team_id,
            title: Some(title),
            description,
            priority,
            assignee_id,
            project_id,
            state_id,
            parent_id,
            label_ids: label_ids_opt,
        };

        let op = IssueCreateMutation::build(IssueCreateArguments { input });
        let resp = client.run(op).await.map_err(to_tool_error)?;
        let data = http::extract_data(resp).map_err(to_tool_error)?;

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
        #[universal_tool_param(description = "Issue ID, identifier (e.g., ENG-245), or URL")]
        issue: String,
        #[universal_tool_param(description = "Comment body (markdown supported)")] body: String,
        #[universal_tool_param(description = "Parent comment ID for replies (UUID)")]
        parent_id: Option<String>,
    ) -> Result<models::CommentResult, ToolError> {
        let client = LinearClient::new(self.api_key.clone()).map_err(to_tool_error)?;
        let issue_id = self.resolve_to_issue_id(&client, &issue).await?;

        let input = CommentCreateInput {
            issue_id,
            body: Some(body),
            parent_id,
        };

        let op = CommentCreateMutation::build(CommentCreateArguments { input });
        let resp = client.run(op).await.map_err(to_tool_error)?;
        let data = http::extract_data(resp).map_err(to_tool_error)?;

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

#[cfg(test)]
mod tests {
    use super::parse_identifier;

    #[test]
    fn parse_plain_uppercase() {
        assert_eq!(parse_identifier("ENG-245"), Some(("ENG".into(), 245)));
    }

    #[test]
    fn parse_lowercase_normalizes() {
        assert_eq!(parse_identifier("eng-245"), Some(("ENG".into(), 245)));
    }

    #[test]
    fn parse_from_url() {
        assert_eq!(
            parse_identifier("https://linear.app/foo/issue/eng-245/slug"),
            Some(("ENG".into(), 245))
        );
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert_eq!(parse_identifier("invalid"), None);
        assert_eq!(parse_identifier("ENG-"), None);
        assert_eq!(parse_identifier("ENG"), None);
        assert_eq!(parse_identifier("123-456"), None);
    }
}
