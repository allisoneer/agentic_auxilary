use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::OnceLock;
use universal_tool_core::mcp::McpFormatter;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewComment {
    pub id: u64,
    pub user: String,
    pub body: String,
    pub path: String,
    pub line: Option<u64>,
    pub side: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub html_url: String,
    pub pull_request_review_id: Option<u64>,
    pub in_reply_to_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueComment {
    pub id: u64,
    pub user: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AllComments {
    pub pr_number: u64,
    pub pr_title: String,
    pub review_comments: Vec<ReviewComment>,
    pub issue_comments: Vec<IssueComment>,
    pub total_comments: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrSummary {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub comment_count: u32,
    pub review_comment_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewCommentList {
    pub comments: Vec<ReviewComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueCommentList {
    pub comments: Vec<IssueComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrSummaryList {
    pub prs: Vec<PrSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct FormatOptions {
    pub show_ids: bool,
    pub show_urls: bool,
    pub show_dates: bool,      // created_at/updated_at
    pub show_review_ids: bool, // pull_request_review_id
    pub show_counts: bool,     // PR list: comment_count, review_comment_count
    pub show_author: bool,     // PR list: author
}

// Cache options once per process
static FORMAT_OPTIONS: OnceLock<FormatOptions> = OnceLock::new();

impl FormatOptions {
    pub fn from_env() -> Self {
        let raw = std::env::var("PR_COMMENTS_EXTRAS").unwrap_or_default();
        Self::from_csv(&raw)
    }

    // Pure helper for testing
    pub fn from_csv(csv: &str) -> Self {
        let mut opts = FormatOptions::default();
        for flag in csv
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
        {
            match flag.as_str() {
                "id" | "ids" => opts.show_ids = true,
                "url" | "urls" => opts.show_urls = true,
                "date" | "dates" | "time" | "times" => opts.show_dates = true,
                "review" | "review_id" | "review_ids" => opts.show_review_ids = true,
                "count" | "counts" => opts.show_counts = true,
                "author" | "authors" => opts.show_author = true,
                _ => {}
            }
        }
        opts
    }

    pub fn get() -> &'static FormatOptions {
        FORMAT_OPTIONS.get_or_init(Self::from_env)
    }
}

// Helpers

pub fn group_by_path(comments: &[ReviewComment]) -> BTreeMap<&str, Vec<&ReviewComment>> {
    let mut map: BTreeMap<&str, Vec<&ReviewComment>> = BTreeMap::new();
    for c in comments {
        map.entry(&c.path).or_default().push(c);
    }
    map
}

pub fn compress_side(side: Option<&str>) -> &'static str {
    match side {
        Some(s) if s.eq_ignore_ascii_case("RIGHT") => "R",
        Some(s) if s.eq_ignore_ascii_case("LEFT") => "L",
        _ => "-",
    }
}

pub fn format_legend() -> &'static str {
    "Legend: L = old (LEFT), R = new (RIGHT), - = unknown"
}

pub fn indent_multiline(s: &str, indent: &str) -> String {
    let mut out = String::new();
    for (i, line) in s.lines().enumerate() {
        if i == 0 {
            out.push_str(line);
        } else {
            out.push('\n');
            out.push_str(indent);
            out.push_str(line);
        }
    }
    out
}

fn fmt_header(title: &str) -> String {
    // simple single-line header
    title.to_string()
}

fn fmt_user(user: &str) -> &str {
    if user.is_empty() { "<unknown>" } else { user }
}

fn fmt_ts(ts: &str) -> &str {
    // Avoid dependency on chrono; use as-is
    ts
}

// McpFormatter implementations

impl McpFormatter for ReviewCommentList {
    fn mcp_format_text(&self) -> String {
        if self.comments.is_empty() {
            return "Review comments: <none>".into();
        }
        let opts = FormatOptions::get();
        let mut out = String::new();
        let _ = writeln!(out, "{}", fmt_header("Review comments:"));
        let _ = writeln!(out, "{}", format_legend());

        let grouped = group_by_path(&self.comments);

        for (path, comments) in grouped {
            let _ = writeln!(out, "\n{}", path);

            // Build replies map
            let mut replies_by_parent: std::collections::BTreeMap<u64, Vec<&ReviewComment>> =
                std::collections::BTreeMap::new();
            for c in comments.iter().filter(|c| c.in_reply_to_id.is_some()) {
                if let Some(pid) = c.in_reply_to_id {
                    replies_by_parent.entry(pid).or_default().push(c);
                }
            }

            // Render top-level comments, then their replies
            for c in comments {
                if c.in_reply_to_id.is_some() {
                    continue; // replies rendered under their parent
                }

                // Format top-level comment
                let side = compress_side(c.side.as_deref());
                let line_disp = c.line.map(|n| n.to_string()).unwrap_or_else(|| "?".into());
                let mut head = format!("  [{} {}] {}", line_disp, side, fmt_user(&c.user));
                if opts.show_ids {
                    head.push_str(&format!(" #{}", c.id));
                }
                if opts.show_review_ids
                    && let Some(rid) = c.pull_request_review_id
                {
                    head.push_str(&format!(" (review:{})", rid));
                }
                if opts.show_urls {
                    head.push_str(&format!(" {}", c.html_url));
                }
                if opts.show_dates {
                    head.push_str(&format!(" @{}", fmt_ts(&c.created_at)));
                }
                let _ = writeln!(out, "{}", head);
                let body = indent_multiline(&c.body, "    ");
                let _ = writeln!(out, "    {}", body);

                // Render replies indented under parent
                if let Some(replies) = replies_by_parent.get(&c.id) {
                    for r in replies {
                        let side_r = compress_side(r.side.as_deref());
                        let line_r = r.line.map(|n| n.to_string()).unwrap_or_else(|| "?".into());
                        let mut head_r =
                            format!("    ↳ [{} {}] {}", line_r, side_r, fmt_user(&r.user));
                        if opts.show_ids {
                            head_r.push_str(&format!(" #{}", r.id));
                        }
                        if opts.show_review_ids
                            && let Some(rid) = r.pull_request_review_id
                        {
                            head_r.push_str(&format!(" (review:{})", rid));
                        }
                        if opts.show_urls {
                            head_r.push_str(&format!(" {}", r.html_url));
                        }
                        if opts.show_dates {
                            head_r.push_str(&format!(" @{}", fmt_ts(&r.created_at)));
                        }
                        let _ = writeln!(out, "{}", head_r);
                        let body_r = indent_multiline(&r.body, "      ");
                        let _ = writeln!(out, "      {}", body_r);
                    }
                }
            }
        }
        out
    }
}

impl McpFormatter for IssueCommentList {
    fn mcp_format_text(&self) -> String {
        if self.comments.is_empty() {
            return "Issue comments: <none>".into();
        }
        let opts = FormatOptions::get();
        let mut out = String::new();
        let _ = writeln!(out, "{}", fmt_header("Issue comments:"));

        for c in &self.comments {
            let mut head = format!("  {}", fmt_user(&c.user));
            if opts.show_ids {
                head.push_str(&format!(" #{}", c.id));
            }
            if opts.show_urls {
                head.push_str(&format!(" {}", c.html_url));
            }
            if opts.show_dates {
                head.push_str(&format!(" @{}", fmt_ts(&c.created_at)));
            }
            let _ = writeln!(out, "{}", head);

            let body = indent_multiline(&c.body, "    ");
            let _ = writeln!(out, "    {}", body);
        }
        out
    }
}

impl McpFormatter for PrSummaryList {
    fn mcp_format_text(&self) -> String {
        if self.prs.is_empty() {
            return "Pull requests: <none>".into();
        }
        let opts = FormatOptions::get();
        let mut out = String::new();
        let _ = writeln!(out, "{}", fmt_header("Pull requests:"));
        for pr in &self.prs {
            let mut line = format!("#{} {} — {}", pr.number, pr.state, pr.title);
            if opts.show_author {
                line.push_str(&format!(" (by {})", pr.author));
            }
            if opts.show_counts {
                line.push_str(&format!(
                    " [comments={}, review_comments={}]",
                    pr.comment_count, pr.review_comment_count
                ));
            }
            if opts.show_dates {
                line.push_str(&format!(" @{}", fmt_ts(&pr.updated_at)));
            }
            let _ = writeln!(out, "{}", line);
        }
        out
    }
}

impl McpFormatter for AllComments {
    fn mcp_format_text(&self) -> String {
        // Compose using the other implementations for consistent display
        let mut out = String::new();
        let _ = writeln!(out, "PR #{}: {}", self.pr_number, self.pr_title);
        let _ = writeln!(
            out,
            "Total comments: {} (review: {}, issue: {})",
            self.total_comments,
            self.review_comments.len(),
            self.issue_comments.len()
        );

        // Reviews
        let r = ReviewCommentList {
            comments: self.review_comments.clone(),
        }
        .mcp_format_text();
        if !r.is_empty() {
            let _ = writeln!(out, "\n{}", r);
        }

        // Issues
        let i = IssueCommentList {
            comments: self.issue_comments.clone(),
        }
        .mcp_format_text();
        if !i.is_empty() {
            let _ = writeln!(out, "\n{}", i);
        }

        out.trim_end().to_string()
    }
}

impl From<octocrab::models::pulls::Comment> for ReviewComment {
    fn from(comment: octocrab::models::pulls::Comment) -> Self {
        Self {
            id: comment.id.0,
            user: comment.user.map(|u| u.login).unwrap_or_default(),
            body: comment.body,
            path: comment.path,
            line: comment.line,
            side: comment.side,
            created_at: comment.created_at.to_rfc3339(),
            updated_at: comment.updated_at.to_rfc3339(),
            html_url: comment.html_url,
            pull_request_review_id: comment.pull_request_review_id.map(|id| id.0),
            in_reply_to_id: comment.in_reply_to_id.map(|id| id.0),
        }
    }
}

impl From<octocrab::models::issues::Comment> for IssueComment {
    fn from(comment: octocrab::models::issues::Comment) -> Self {
        Self {
            id: comment.id.0,
            user: comment.user.login,
            body: comment.body.unwrap_or_default(),
            created_at: comment.created_at.to_rfc3339(),
            updated_at: comment.updated_at.map(|dt| dt.to_rfc3339()),
            html_url: comment.html_url.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLError {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestData {
    pub repository: Repository,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    #[serde(rename = "pullRequest")]
    pub pull_request: PullRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    #[serde(rename = "reviewThreads")]
    pub review_threads: ReviewThreadConnection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThreadConnection {
    pub nodes: Vec<ReviewThread>,
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThread {
    pub id: String,
    #[serde(rename = "isResolved")]
    pub is_resolved: bool,
    pub comments: ReviewThreadCommentConnection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThreadCommentConnection {
    pub nodes: Vec<ReviewThreadComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThreadComment {
    pub id: String,
    #[serde(rename = "databaseId")]
    pub database_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    #[serde(rename = "hasNextPage")]
    pub has_next_page: bool,
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
}

#[cfg(test)]
mod format_options_tests {
    use super::FormatOptions;

    #[test]
    fn parses_empty_flags() {
        let o = FormatOptions::from_csv("");
        assert!(
            !o.show_ids
                && !o.show_urls
                && !o.show_dates
                && !o.show_review_ids
                && !o.show_counts
                && !o.show_author
        );
    }

    #[test]
    fn parses_known_flags_with_synonyms() {
        let o = FormatOptions::from_csv("id, url, dates, review, counts, author");
        assert!(
            o.show_ids
                && o.show_urls
                && o.show_dates
                && o.show_review_ids
                && o.show_counts
                && o.show_author
        );

        let o2 = FormatOptions::from_csv("ids, urls, times, review_id, count, authors");
        assert!(
            o2.show_ids
                && o2.show_urls
                && o2.show_dates
                && o2.show_review_ids
                && o2.show_counts
                && o2.show_author
        );
    }
}

#[cfg(test)]
mod mcp_format_tests {
    use super::*;

    fn sample_review(
        path: &str,
        line: Option<u64>,
        side: Option<&str>,
        user: &str,
        body: &str,
    ) -> ReviewComment {
        ReviewComment {
            id: 1,
            user: user.into(),
            body: body.into(),
            path: path.into(),
            line,
            side: side.map(|s| s.into()),
            created_at: "2025-01-01T00:00:00Z".into(),
            updated_at: "2025-01-01T00:00:00Z".into(),
            html_url: "https://example.com/review/1".into(),
            pull_request_review_id: Some(42),
            in_reply_to_id: None,
        }
    }

    fn sample_issue(user: &str, body: &str) -> IssueComment {
        IssueComment {
            id: 10,
            user: user.into(),
            body: body.into(),
            created_at: "2025-01-01T00:00:00Z".into(),
            updated_at: None,
            html_url: "https://example.com/issue/10".into(),
        }
    }

    #[test]
    fn group_by_path_groups_and_orders() {
        let cs = vec![
            sample_review("a.rs", Some(1), Some("RIGHT"), "u", "x"),
            sample_review("b.rs", Some(2), Some("LEFT"), "u", "y"),
            sample_review("a.rs", None, None, "u2", "z"),
        ];
        let g = group_by_path(&cs);
        let keys: Vec<_> = g.keys().cloned().collect();
        assert_eq!(keys, vec!["a.rs", "b.rs"]);
        assert_eq!(g["a.rs"].len(), 2);
        assert_eq!(g["b.rs"].len(), 1);
    }

    #[test]
    fn side_compression() {
        assert_eq!(compress_side(Some("RIGHT")), "R");
        assert_eq!(compress_side(Some("LEFT")), "L");
        assert_eq!(compress_side(Some("right")), "R");
        assert_eq!(compress_side(None), "-");
    }

    #[test]
    fn indent_multiline_preserves_and_indents() {
        let s = "line1\nline2\nline3";
        let out = indent_multiline(s, "  ");
        assert!(out.starts_with("line1"));
        assert!(out.contains("\n  line2"));
        assert!(out.ends_with("\n  line3"));
    }

    #[test]
    fn format_review_comment_list_basic() {
        unsafe {
            std::env::remove_var("PR_COMMENTS_EXTRAS");
        }
        let list = ReviewCommentList {
            comments: vec![
                sample_review(
                    "src/lib.rs",
                    Some(12),
                    Some("RIGHT"),
                    "alice",
                    "Body A\nMore",
                ),
                sample_review("src/lib.rs", Some(42), Some("LEFT"), "bob", "Body B"),
            ],
        };
        let text = list.mcp_format_text();
        assert!(text.contains("Review comments:"));
        assert!(text.contains("Legend:"));
        assert!(text.contains("src/lib.rs"));
        assert!(text.contains("[12 R] alice"));
        assert!(text.contains("[42 L] bob"));
        assert!(text.contains("Body A"));
        assert!(text.contains("\n    More"));
        assert!(!text.contains("#1")); // ids are off by default
    }

    #[test]
    fn format_issue_comment_list_basic() {
        unsafe {
            std::env::remove_var("PR_COMMENTS_EXTRAS");
        }
        let list = IssueCommentList {
            comments: vec![
                sample_issue("charlie", "Hello\nWorld"),
                sample_issue("", "No user"),
            ],
        };
        let text = list.mcp_format_text();
        assert!(text.contains("Issue comments:"));
        assert!(text.contains("charlie"));
        assert!(text.contains("<unknown>"));
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn format_pr_summary_list_basic() {
        unsafe {
            std::env::remove_var("PR_COMMENTS_EXTRAS");
        }
        let list = PrSummaryList {
            prs: vec![PrSummary {
                number: 123,
                title: "Fix bug".into(),
                author: "dana".into(),
                state: "open".into(),
                created_at: "2025-01-01T00:00:00Z".into(),
                updated_at: "2025-01-02T00:00:00Z".into(),
                comment_count: 2,
                review_comment_count: 3,
            }],
        };
        let text = list.mcp_format_text();
        assert!(text.contains("Pull requests:"));
        assert!(text.contains("#123 open — Fix bug"));
        assert!(!text.contains("comments=")); // counts off by default
        assert!(!text.contains("(by ")); // author off by default
    }

    #[test]
    fn format_all_comments_composes() {
        let ac = AllComments {
            pr_number: 5,
            pr_title: "Great PR".into(),
            review_comments: vec![sample_review("a.rs", Some(1), Some("RIGHT"), "u", "b")],
            issue_comments: vec![sample_issue("v", "c")],
            total_comments: 2,
        };
        let text = ac.mcp_format_text();
        assert!(text.contains("PR #5: Great PR"));
        assert!(text.contains("Total comments: 2 (review: 1, issue: 1)"));
        assert!(text.contains("Review comments:"));
        assert!(text.contains("Issue comments:"));
    }

    #[test]
    fn extras_flags_parsing() {
        // Test that from_csv correctly enables all optional fields
        let opts = FormatOptions::from_csv("id,url,dates,review,counts,author");
        assert!(opts.show_ids);
        assert!(opts.show_urls);
        assert!(opts.show_dates);
        assert!(opts.show_review_ids);
        assert!(opts.show_counts);
        assert!(opts.show_author);

        // Test default has all disabled
        let default_opts = FormatOptions::from_csv("");
        assert!(!default_opts.show_ids);
        assert!(!default_opts.show_urls);
        assert!(!default_opts.show_dates);
        assert!(!default_opts.show_review_ids);
        assert!(!default_opts.show_counts);
        assert!(!default_opts.show_author);
    }

    #[test]
    fn wrapper_serializes_as_object() {
        let w = ReviewCommentList { comments: vec![] };
        let s = serde_json::to_string(&w).unwrap();
        assert!(s.contains("\"comments\""));
    }
}
