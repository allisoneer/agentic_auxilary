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
    reviews.iter().any(|review| {
        review
            .user_login
            .to_ascii_lowercase()
            .contains("coderabbit")
            || review.user_type.as_deref() == Some("Bot")
    })
}

fn interpret_issue_comments(comments: &[IssueCommentSummary]) -> CodeRabbitPoll {
    for comment in comments {
        let body = comment.body.to_ascii_lowercase();
        let is_coderabbit = comment
            .user_login
            .to_ascii_lowercase()
            .contains("coderabbit")
            || comment.user_type.as_deref() == Some("Bot");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_skipped_comment() {
        let result = interpret_issue_comments(&[IssueCommentSummary {
            id: 1,
            user_login: "coderabbitai[bot]".to_string(),
            user_type: Some("Bot".to_string()),
            body: "Review skipped because no actionable changes were found".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }]);
        assert!(matches!(result, CodeRabbitPoll::Skipped { .. }));
    }
}
