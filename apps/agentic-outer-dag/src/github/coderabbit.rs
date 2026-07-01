use anyhow::Result;
use pr_comments::PrComments;
use pr_comments::models::CheckSuiteSummary;
use pr_comments::models::IssueCommentSummary;
use pr_comments::models::PullRequestReviewSummary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeRabbitPoll {
    Waiting,
    Completed,
    Skipped { reason: String },
}

pub struct CodeRabbitClient {
    pr_comments: PrComments,
}

impl CodeRabbitClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            pr_comments: PrComments::new()?,
        })
    }

    pub async fn poll_once(&self, pr_number: u64, head_sha: &str) -> Result<CodeRabbitPoll> {
        let suites = self.pr_comments.list_check_suites_for_ref(head_sha).await?;
        let suites_outcome = interpret_check_suites(&suites);
        if matches!(suites_outcome, Some(CodeRabbitPoll::Completed)) {
            return Ok(CodeRabbitPoll::Completed);
        }

        let reviews = self
            .pr_comments
            .list_pull_request_reviews(pr_number)
            .await?;
        if has_coderabbit_review_for_head(&reviews, head_sha) {
            return Ok(CodeRabbitPoll::Completed);
        }

        let comments = self.pr_comments.list_issue_comments(pr_number).await?;
        if let Some(reason) = interpret_issue_comments_for_skip(&comments) {
            return Ok(CodeRabbitPoll::Skipped { reason });
        }

        Ok(suites_outcome.unwrap_or(CodeRabbitPoll::Waiting))
    }
}

fn interpret_check_suites(suites: &[CheckSuiteSummary]) -> Option<CodeRabbitPoll> {
    let coderabbit: Vec<_> = suites
        .iter()
        .filter(|suite| suite.app_slug.as_deref() == Some("coderabbitai"))
        .collect();
    if coderabbit.is_empty() {
        return None;
    }
    if coderabbit.iter().any(|suite| suite.status == "completed") {
        Some(CodeRabbitPoll::Completed)
    } else {
        Some(CodeRabbitPoll::Waiting)
    }
}

fn has_coderabbit_review_for_head(reviews: &[PullRequestReviewSummary], head_sha: &str) -> bool {
    reviews.iter().any(|review| {
        is_coderabbit_login(&review.user_login) && review.commit_id.as_deref() == Some(head_sha)
    })
}

fn is_coderabbit_login(login: &str) -> bool {
    login.to_ascii_lowercase().contains("coderabbit")
}

fn interpret_issue_comments_for_skip(comments: &[IssueCommentSummary]) -> Option<String> {
    for comment in comments {
        let body = comment.body.to_ascii_lowercase();
        let is_coderabbit = is_coderabbit_login(&comment.user_login);
        if is_coderabbit && body.contains("review skipped") {
            return Some(comment.body.clone());
        }
    }

    None
}

pub fn skip_reason_indicates_draft(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    reason.contains("draft detected")
        || reason.contains("draft pull request")
        || reason.contains("pr is draft")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue_comment(user_login: &str, body: &str) -> IssueCommentSummary {
        IssueCommentSummary {
            id: 1,
            user_login: user_login.to_string(),
            user_type: Some("Bot".to_string()),
            body: body.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn review(user_login: &str, commit_id: Option<&str>) -> PullRequestReviewSummary {
        PullRequestReviewSummary {
            id: 1,
            user_login: user_login.to_string(),
            user_type: Some("Bot".to_string()),
            state: "COMMENTED".to_string(),
            submitted_at: Some("2026-01-01T00:00:00Z".to_string()),
            commit_id: commit_id.map(str::to_string),
        }
    }

    fn check_suite(
        status: &str,
        conclusion: Option<&str>,
        app_slug: Option<&str>,
    ) -> CheckSuiteSummary {
        CheckSuiteSummary {
            id: 1,
            status: status.to_string(),
            conclusion: conclusion.map(str::to_string),
            app_slug: app_slug.map(str::to_string),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn matches_coderabbit_logins_case_insensitively() {
        for login in [
            "coderabbitai",
            "coderabbitai[bot]",
            "CodeRabbitAI",
            "my-coderabbit-helper",
        ] {
            assert!(is_coderabbit_login(login), "expected match for {login}");
        }
    }

    #[test]
    fn rejects_unrelated_bot_logins() {
        for login in ["dependabot[bot]", "renovate[bot]"] {
            assert!(!is_coderabbit_login(login), "unexpected match for {login}");
        }
    }

    #[test]
    fn interpret_check_suites_returns_none_without_coderabbit_suite() {
        assert_eq!(
            interpret_check_suites(&[check_suite("queued", None, Some("github-actions"))]),
            None
        );
    }

    #[test]
    fn interpret_check_suites_treats_queued_and_in_progress_as_waiting() {
        for status in ["queued", "in_progress"] {
            assert_eq!(
                interpret_check_suites(&[check_suite(status, None, Some("coderabbitai"))]),
                Some(CodeRabbitPoll::Waiting)
            );
        }
    }

    #[test]
    fn interpret_check_suites_treats_completed_success_as_completed() {
        assert_eq!(
            interpret_check_suites(&[check_suite(
                "completed",
                Some("success"),
                Some("coderabbitai")
            )]),
            Some(CodeRabbitPoll::Completed)
        );
    }

    #[test]
    fn interpret_check_suites_keeps_completed_non_success_as_completed_for_current_policy() {
        assert_eq!(
            interpret_check_suites(&[check_suite(
                "completed",
                Some("failure"),
                Some("coderabbitai")
            )]),
            Some(CodeRabbitPoll::Completed)
        );
    }

    #[test]
    fn coderabbit_review_completion_requires_matching_head_sha() {
        assert!(has_coderabbit_review_for_head(
            &[review("coderabbitai[bot]", Some("abc123"))],
            "abc123"
        ));
        assert!(!has_coderabbit_review_for_head(
            &[review("coderabbitai[bot]", Some("deadbeef"))],
            "abc123"
        ));
        assert!(!has_coderabbit_review_for_head(
            &[review("dependabot[bot]", Some("abc123"))],
            "abc123"
        ));
        assert!(!has_coderabbit_review_for_head(
            &[review("coderabbitai[bot]", None)],
            "abc123"
        ));
    }

    #[test]
    fn detects_skipped_comment() {
        let result = interpret_issue_comments_for_skip(&[issue_comment(
            "coderabbitai[bot]",
            "Review skipped because no actionable changes were found",
        )]);
        assert_eq!(
            result.as_deref(),
            Some("Review skipped because no actionable changes were found")
        );
    }

    #[test]
    fn unrelated_bot_comment_does_not_complete_poll() {
        let result = interpret_issue_comments_for_skip(&[issue_comment(
            "dependabot[bot]",
            "Review skipped because no actionable changes were found",
        )]);
        assert!(result.is_none());
    }

    #[test]
    fn coderabbit_issue_comment_does_not_complete_without_skip_phrase() {
        let result =
            interpret_issue_comments_for_skip(&[issue_comment("CodeRabbitAI", "Looks good to me")]);
        assert!(result.is_none());
    }

    #[test]
    fn detects_draft_skip_reason_variants() {
        assert!(skip_reason_indicates_draft(
            "Review skipped. Draft detected."
        ));
        assert!(skip_reason_indicates_draft(
            "review skipped because PR is draft"
        ));
        assert!(!skip_reason_indicates_draft(
            "Review skipped because no actionable changes were found"
        ));
    }
}
