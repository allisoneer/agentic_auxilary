pub mod http;
pub mod models;
pub mod tools;

/// Test support utilities (for use in tests)
#[doc(hidden)]
pub mod test_support;

use anyhow::{Context, Result};
use cynic::{MutationBuilder, QueryBuilder};
use http::LinearClient;
use linear_queries::scalars::DateTimeOrDuration;
use linear_queries::*;
use regex::Regex;

// Re-export agentic-tools types for MCP server usage
pub use tools::build_registry;

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
    async fn resolve_to_issue_id(&self, client: &LinearClient, input: &str) -> Result<String> {
        match self.resolve_issue_id(input) {
            IssueIdentifier::Id(id) => Ok(id),
            IssueIdentifier::Identifier(ident) => {
                let (team_key, number) = parse_identifier(&ident)
                    .ok_or_else(|| anyhow::anyhow!("not found: Issue {} not found", ident))?;
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
                let resp = client.run(op).await?;
                let data = http::extract_data(resp)?;
                let issue = data
                    .issues
                    .nodes
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("not found: Issue {} not found", ident))?;
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

// Note: Error handling moved to tools.rs with map_anyhow_to_tool_error.
// HTTP error enrichment via summarize_reqwest_error is available for use in tools.rs if needed.

// ============================================================================
// From impls: GraphQL types -> tool model types
// ============================================================================

impl From<linear_queries::User> for models::UserRef {
    fn from(u: linear_queries::User) -> Self {
        let name = if u.display_name.is_empty() {
            u.name
        } else {
            u.display_name
        };
        Self {
            id: u.id.inner().to_string(),
            name,
            email: u.email,
        }
    }
}

impl From<linear_queries::Team> for models::TeamRef {
    fn from(t: linear_queries::Team) -> Self {
        Self {
            id: t.id.inner().to_string(),
            key: t.key,
            name: t.name,
        }
    }
}

impl From<linear_queries::WorkflowState> for models::WorkflowStateRef {
    fn from(s: linear_queries::WorkflowState) -> Self {
        Self {
            id: s.id.inner().to_string(),
            name: s.name,
            state_type: s.state_type,
        }
    }
}

impl From<linear_queries::Project> for models::ProjectRef {
    fn from(p: linear_queries::Project) -> Self {
        Self {
            id: p.id.inner().to_string(),
            name: p.name,
        }
    }
}

impl From<linear_queries::ParentIssue> for models::ParentIssueRef {
    fn from(p: linear_queries::ParentIssue) -> Self {
        Self {
            id: p.id.inner().to_string(),
            identifier: p.identifier,
        }
    }
}

impl From<linear_queries::Issue> for models::IssueSummary {
    fn from(i: linear_queries::Issue) -> Self {
        Self {
            id: i.id.inner().to_string(),
            identifier: i.identifier,
            title: i.title,
            url: i.url,
            team: i.team.into(),
            state: i.state.map(Into::into),
            assignee: i.assignee.map(Into::into),
            creator: i.creator.map(Into::into),
            project: i.project.map(Into::into),
            priority: i.priority as i32,
            priority_label: i.priority_label,
            label_ids: i.label_ids,
            due_date: i.due_date.map(|d| d.0),
            created_at: i.created_at.0,
            updated_at: i.updated_at.0,
        }
    }
}

impl From<linear_queries::IssueSearchResult> for models::IssueSummary {
    fn from(i: linear_queries::IssueSearchResult) -> Self {
        Self {
            id: i.id.inner().to_string(),
            identifier: i.identifier,
            title: i.title,
            url: i.url,
            team: i.team.into(),
            state: Some(i.state.into()),
            assignee: i.assignee.map(Into::into),
            creator: i.creator.map(Into::into),
            project: i.project.map(Into::into),
            priority: i.priority as i32,
            priority_label: i.priority_label,
            label_ids: i.label_ids,
            due_date: i.due_date.map(|d| d.0),
            created_at: i.created_at.0,
            updated_at: i.updated_at.0,
        }
    }
}

// Removed universal-tool-core macros; Tool impls live in tools.rs
impl LinearTools {
    /// Search Linear issues with full-text search or filters
    #[allow(clippy::too_many_arguments)]
    pub async fn search_issues(
        &self,
        query: Option<String>,
        include_comments: Option<bool>,
        priority: Option<i32>,
        state_id: Option<String>,
        assignee_id: Option<String>,
        team_id: Option<String>,
        project_id: Option<String>,
        created_after: Option<String>,
        created_before: Option<String>,
        updated_after: Option<String>,
        updated_before: Option<String>,
        first: Option<i32>,
        after: Option<String>,
    ) -> Result<models::SearchResult> {
        let client = LinearClient::new(self.api_key.clone())
            .context("internal: failed to create Linear client")?;

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
                ..Default::default()
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
            let resp = client.run(op).await?;
            let data = http::extract_data(resp)?;

            let issues = data
                .search_issues
                .nodes
                .into_iter()
                .map(Into::into)
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

            let resp = client.run(op).await?;
            let data = http::extract_data(resp)?;

            let issues = data.issues.nodes.into_iter().map(Into::into).collect();

            Ok(models::SearchResult {
                issues,
                has_next_page: data.issues.page_info.has_next_page,
                end_cursor: data.issues.page_info.end_cursor,
            })
        }
    }

    /// Read a single Linear issue
    pub async fn read_issue(&self, issue: String) -> Result<models::IssueDetails> {
        let client = LinearClient::new(self.api_key.clone())
            .context("internal: failed to create Linear client")?;
        let resolved = self.resolve_issue_id(&issue);

        let issue_data = match resolved {
            IssueIdentifier::Id(id) => {
                let op = IssueByIdQuery::build(IssueByIdArguments { id });
                let resp = client.run(op).await?;
                let data = http::extract_data(resp)?;
                data.issue
                    .ok_or_else(|| anyhow::anyhow!("not found: Issue not found"))?
            }
            IssueIdentifier::Identifier(ident) => {
                // Use server-side filtering by team.key + number
                let (team_key, number) = parse_identifier(&ident)
                    .ok_or_else(|| anyhow::anyhow!("not found: Issue {} not found", ident))?;
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
                let resp = client.run(op).await?;
                let data = http::extract_data(resp)?;
                data.issues
                    .nodes
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("not found: Issue {} not found", ident))?
            }
        };

        let description = issue_data.description.clone();
        let estimate = issue_data.estimate;
        let started_at = issue_data.started_at.as_ref().map(|d| d.0.clone());
        let completed_at = issue_data.completed_at.as_ref().map(|d| d.0.clone());
        let canceled_at = issue_data.canceled_at.as_ref().map(|d| d.0.clone());
        let parent = issue_data.parent.as_ref().map(|p| models::ParentIssueRef {
            id: p.id.inner().to_string(),
            identifier: p.identifier.clone(),
        });

        let summary: models::IssueSummary = issue_data.into();

        Ok(models::IssueDetails {
            issue: summary,
            description,
            estimate,
            parent,
            started_at,
            completed_at,
            canceled_at,
        })
    }

    /// Create a new Linear issue
    #[allow(clippy::too_many_arguments)]
    pub async fn create_issue(
        &self,
        team_id: String,
        title: String,
        description: Option<String>,
        priority: Option<i32>,
        assignee_id: Option<String>,
        project_id: Option<String>,
        state_id: Option<String>,
        parent_id: Option<String>,
        label_ids: Vec<String>,
    ) -> Result<models::CreateIssueResult> {
        let client = LinearClient::new(self.api_key.clone())
            .context("internal: failed to create Linear client")?;

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
        let resp = client.run(op).await?;
        let data = http::extract_data(resp)?;

        let payload = data.issue_create;
        let issue: Option<models::IssueSummary> = payload.issue.map(Into::into);

        Ok(models::CreateIssueResult {
            success: payload.success,
            issue,
        })
    }

    /// Add a comment to a Linear issue
    pub async fn add_comment(
        &self,
        issue: String,
        body: String,
        parent_id: Option<String>,
    ) -> Result<models::CommentResult> {
        let client = LinearClient::new(self.api_key.clone())
            .context("internal: failed to create Linear client")?;
        let issue_id = self.resolve_to_issue_id(&client, &issue).await?;

        let input = CommentCreateInput {
            issue_id,
            body: Some(body),
            parent_id,
        };

        let op = CommentCreateMutation::build(CommentCreateArguments { input });
        let resp = client.run(op).await?;
        let data = http::extract_data(resp)?;

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

    /// Archive a Linear issue
    pub async fn archive_issue(&self, issue: String) -> Result<models::ArchiveIssueResult> {
        let client = LinearClient::new(self.api_key.clone())
            .context("internal: failed to create Linear client")?;
        let id = self.resolve_to_issue_id(&client, &issue).await?;
        let op = IssueArchiveMutation::build(IssueArchiveArguments { id });
        let resp = client.run(op).await?;
        let data = http::extract_data(resp)?;
        Ok(models::ArchiveIssueResult {
            success: data.issue_archive.success,
        })
    }

    /// Get metadata (users, teams, projects, workflow states, or labels)
    pub async fn get_metadata(
        &self,
        kind: models::MetadataKind,
        search: Option<String>,
        team_id: Option<String>,
        first: Option<i32>,
        after: Option<String>,
    ) -> Result<models::GetMetadataResult> {
        let client = LinearClient::new(self.api_key.clone())
            .context("internal: failed to create Linear client")?;
        let first = first.or(Some(50));

        match kind {
            models::MetadataKind::Users => {
                let filter = search.map(|s| linear_queries::UserFilter {
                    display_name: Some(StringComparator {
                        contains_ignore_case: Some(s),
                        ..Default::default()
                    }),
                });
                let op = linear_queries::UsersQuery::build(linear_queries::UsersArguments {
                    first,
                    after,
                    filter,
                });
                let resp = client.run(op).await?;
                let data = http::extract_data(resp)?;
                let items = data
                    .users
                    .nodes
                    .into_iter()
                    .map(|u| {
                        let name = if u.display_name.is_empty() {
                            u.name
                        } else {
                            u.display_name
                        };
                        models::MetadataItem {
                            id: u.id.inner().to_string(),
                            name,
                            email: Some(u.email),
                            key: None,
                            state_type: None,
                            team_id: None,
                        }
                    })
                    .collect();
                Ok(models::GetMetadataResult {
                    kind: models::MetadataKind::Users,
                    items,
                    has_next_page: data.users.page_info.has_next_page,
                    end_cursor: data.users.page_info.end_cursor,
                })
            }
            models::MetadataKind::Teams => {
                let filter = search.map(|s| linear_queries::TeamFilter {
                    key: Some(StringComparator {
                        contains_ignore_case: Some(s),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                let op = linear_queries::TeamsQuery::build(linear_queries::TeamsArguments {
                    first,
                    after,
                    filter,
                });
                let resp = client.run(op).await?;
                let data = http::extract_data(resp)?;
                let items = data
                    .teams
                    .nodes
                    .into_iter()
                    .map(|t| models::MetadataItem {
                        id: t.id.inner().to_string(),
                        name: t.name,
                        key: Some(t.key),
                        email: None,
                        state_type: None,
                        team_id: None,
                    })
                    .collect();
                Ok(models::GetMetadataResult {
                    kind: models::MetadataKind::Teams,
                    items,
                    has_next_page: data.teams.page_info.has_next_page,
                    end_cursor: data.teams.page_info.end_cursor,
                })
            }
            models::MetadataKind::Projects => {
                let filter = search.map(|s| linear_queries::ProjectFilter {
                    name: Some(StringComparator {
                        contains_ignore_case: Some(s),
                        ..Default::default()
                    }),
                });
                let op = linear_queries::ProjectsQuery::build(linear_queries::ProjectsArguments {
                    first,
                    after,
                    filter,
                });
                let resp = client.run(op).await?;
                let data = http::extract_data(resp)?;
                let items = data
                    .projects
                    .nodes
                    .into_iter()
                    .map(|p| models::MetadataItem {
                        id: p.id.inner().to_string(),
                        name: p.name,
                        key: None,
                        email: None,
                        state_type: None,
                        team_id: None,
                    })
                    .collect();
                Ok(models::GetMetadataResult {
                    kind: models::MetadataKind::Projects,
                    items,
                    has_next_page: data.projects.page_info.has_next_page,
                    end_cursor: data.projects.page_info.end_cursor,
                })
            }
            models::MetadataKind::WorkflowStates => {
                let mut filter = linear_queries::WorkflowStateFilter::default();
                let mut has_filter = false;
                if let Some(s) = search {
                    filter.name = Some(StringComparator {
                        contains_ignore_case: Some(s),
                        ..Default::default()
                    });
                    has_filter = true;
                }
                if let Some(tid) = team_id {
                    filter.team = Some(linear_queries::TeamFilter {
                        id: Some(linear_queries::IdComparator {
                            eq: Some(cynic::Id::new(tid)),
                        }),
                        ..Default::default()
                    });
                    has_filter = true;
                }
                let filter_opt = if has_filter { Some(filter) } else { None };
                let op = linear_queries::WorkflowStatesQuery::build(
                    linear_queries::WorkflowStatesArguments {
                        first,
                        after,
                        filter: filter_opt,
                    },
                );
                let resp = client.run(op).await?;
                let data = http::extract_data(resp)?;
                let items = data
                    .workflow_states
                    .nodes
                    .into_iter()
                    .map(|s| models::MetadataItem {
                        id: s.id.inner().to_string(),
                        name: s.name,
                        state_type: Some(s.state_type),
                        key: None,
                        email: None,
                        team_id: None,
                    })
                    .collect();
                Ok(models::GetMetadataResult {
                    kind: models::MetadataKind::WorkflowStates,
                    items,
                    has_next_page: data.workflow_states.page_info.has_next_page,
                    end_cursor: data.workflow_states.page_info.end_cursor,
                })
            }
            models::MetadataKind::Labels => {
                let mut filter = linear_queries::IssueLabelFilter::default();
                let mut has_filter = false;
                if let Some(s) = search {
                    filter.name = Some(StringComparator {
                        contains_ignore_case: Some(s),
                        ..Default::default()
                    });
                    has_filter = true;
                }
                if let Some(tid) = team_id {
                    filter.team = Some(linear_queries::NullableTeamFilter {
                        id: Some(linear_queries::IdComparator {
                            eq: Some(cynic::Id::new(tid)),
                        }),
                        ..Default::default()
                    });
                    has_filter = true;
                }
                let filter_opt = if has_filter { Some(filter) } else { None };
                let op =
                    linear_queries::IssueLabelsQuery::build(linear_queries::IssueLabelsArguments {
                        first,
                        after,
                        filter: filter_opt,
                    });
                let resp = client.run(op).await?;
                let data = http::extract_data(resp)?;
                let items = data
                    .issue_labels
                    .nodes
                    .into_iter()
                    .map(|l| models::MetadataItem {
                        id: l.id.inner().to_string(),
                        name: l.name,
                        team_id: l.team.map(|t| t.id.inner().to_string()),
                        key: None,
                        email: None,
                        state_type: None,
                    })
                    .collect();
                Ok(models::GetMetadataResult {
                    kind: models::MetadataKind::Labels,
                    items,
                    has_next_page: data.issue_labels.page_info.has_next_page,
                    end_cursor: data.issue_labels.page_info.end_cursor,
                })
            }
        }
    }
}

// Removed universal-tool-core MCP server; use ToolRegistry in tools.rs

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
