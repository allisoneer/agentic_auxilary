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
        if let Some(outcome) = interpret_check_suites(&suites) {
            return Ok(outcome);
        }

        let reviews = self
            .pr_comments
            .list_pull_request_reviews(pr_number)
            .await?;
        if has_coderabbit_review(&reviews) {
            return Ok(CodeRabbitPoll::Completed);
        }

        let comments = self.pr_comments.list_issue_comments(pr_number).await?;
        Ok(interpret_issue_comments(&comments))
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

fn has_coderabbit_review(reviews: &[PullRequestReviewSummary]) -> bool {
    reviews
        .iter()
        .any(|review| is_coderabbit_login(&review.user_login))
}

fn is_coderabbit_login(login: &str) -> bool {
    login.to_ascii_lowercase().contains("coderabbit")
}

fn interpret_issue_comments(comments: &[IssueCommentSummary]) -> CodeRabbitPoll {
    for comment in comments {
        let body = comment.body.to_ascii_lowercase();
        let is_coderabbit = is_coderabbit_login(&comment.user_login);
        if is_coderabbit && body.contains("review skipped") {
            return CodeRabbitPoll::Skipped {
                reason: comment.body.clone(),
            };
        }
        if is_coderabbit {
            return CodeRabbitPoll::Completed;
        }
    }

    CodeRabbitPoll::Waiting
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

    fn review(user_login: &str) -> PullRequestReviewSummary {
        PullRequestReviewSummary {
            id: 1,
            user_login: user_login.to_string(),
            user_type: Some("Bot".to_string()),
            state: "COMMENTED".to_string(),
            submitted_at: Some("2026-01-01T00:00:00Z".to_string()),
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
    fn coderabbit_review_detection_requires_coderabbit_login() {
        assert!(has_coderabbit_review(&[review("coderabbitai[bot]")]));
        assert!(!has_coderabbit_review(&[review("dependabot[bot]")]));
    }

    #[test]
    fn detects_skipped_comment() {
        let result = interpret_issue_comments(&[issue_comment(
            "coderabbitai[bot]",
            "Review skipped because no actionable changes were found",
        )]);
        assert!(matches!(result, CodeRabbitPoll::Skipped { .. }));
    }

    #[test]
    fn unrelated_bot_comment_does_not_complete_poll() {
        let result = interpret_issue_comments(&[issue_comment(
            "dependabot[bot]",
            "Review skipped because no actionable changes were found",
        )]);
        assert_eq!(result, CodeRabbitPoll::Waiting);
    }

    #[test]
    fn coderabbit_comment_completes_poll() {
        let result = interpret_issue_comments(&[issue_comment("CodeRabbitAI", "Looks good to me")]);
        assert_eq!(result, CodeRabbitPoll::Completed);
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
