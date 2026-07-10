use crate::state::RunState;
use anyhow::Context;
use anyhow::Result;
use gwt_worktree::types::BranchName;
use linear_tools::LinearTools;
use sha2::Digest;
use sha2::Sha256;

pub async fn require_issue_branch_name_for_start(ticket: &str) -> Result<String> {
    let tools = LinearTools::new();

    let branch = tools
        .read_issue_branch_name(ticket.to_string())
        .await
        .with_context(|| {
            format!(
                "--branch omitted: failed to read Linear Issue.branchName for ticket '{ticket}'. Set LINEAR_API_KEY (and optionally LINEAR_GRAPHQL_URL) or rerun with --branch <branch> / --worktree <path>."
            )
        })?;

    validate_linear_branch_name(ticket, &branch)
}

fn validate_linear_branch_name(ticket: &str, branch: &str) -> Result<String> {
    let branch = branch.trim();

    anyhow::ensure!(
        !branch.is_empty(),
        "Linear Issue.branchName for ticket '{ticket}' is empty; set the issue's branch name in Linear or pass --branch explicitly"
    );

    let lower = branch.to_ascii_lowercase();
    anyhow::ensure!(
        lower != "main" && lower != "master",
        "Linear Issue.branchName for ticket '{ticket}' resolved to '{branch}', which is not allowed. Update the Linear issue branch name or pass --branch to override."
    );

    BranchName::new(branch.to_string()).map_err(|err| {
        anyhow::anyhow!(
            "Linear Issue.branchName '{branch}' for ticket '{ticket}' is not a valid git branch name: {err}"
        )
    })?;

    Ok(branch.to_string())
}

pub async fn post_handoff_once(state: &mut RunState, message: &str) -> Result<()> {
    if !state.settings.linear_handoff_enabled {
        return Ok(());
    }

    post_handoff_once_unchecked(state, message).await
}

pub async fn post_handoff_once_forced(state: &mut RunState, message: &str) -> Result<()> {
    post_handoff_once_unchecked(state, message).await
}

async fn post_handoff_once_unchecked(state: &mut RunState, message: &str) -> Result<()> {
    let digest = handoff_digest(message);
    if should_skip_handoff(state, &digest) {
        return Ok(());
    }

    let tools = LinearTools::new();
    tools
        .add_comment(state.ticket.linear_key.clone(), message.to_string(), None)
        .await?;
    state.handoff.linear_comment_posted = true;
    state.handoff.linear_comment_body_sha256 = Some(digest);
    state.handoff.posted_at = Some(chrono::Utc::now().to_rfc3339());
    Ok(())
}

fn handoff_digest(message: &str) -> String {
    format!("{:x}", Sha256::digest(message.as_bytes()))
}

fn should_skip_handoff(state: &RunState, digest: &str) -> bool {
    state.handoff.linear_comment_posted
        && state.handoff.linear_comment_body_sha256.as_deref() == Some(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_linear_branch_name_rejects_empty_and_main_like_values() {
        let empty = validate_linear_branch_name("ENG-992", "   ")
            .expect_err("empty branch names should be rejected");
        assert!(empty.to_string().contains("is empty"));

        let main = validate_linear_branch_name("ENG-992", "main")
            .expect_err("main branch names should be rejected");
        assert!(main.to_string().contains("not allowed"));

        let master = validate_linear_branch_name("ENG-992", "MASTER")
            .expect_err("master branch names should be rejected");
        assert!(master.to_string().contains("not allowed"));
    }

    #[test]
    fn validate_linear_branch_name_rejects_invalid_git_branch_names() {
        let err = validate_linear_branch_name("ENG-992", "feature/../bad")
            .expect_err("invalid git branch names should be rejected");
        assert!(err.to_string().contains("is not a valid git branch name"));
    }

    #[test]
    fn handoff_skips_when_same_digest_was_already_posted() {
        let digest = handoff_digest("same");
        let mut state = crate::state::RunState::for_start(
            "ENG-992",
            &crate::worktree::TargetWorktree {
                path: std::env::current_dir().unwrap(),
                branch: "feature/eng-992".to_string(),
                base_ref: "origin/main".to_string(),
            },
            true,
        )
        .unwrap();
        state.handoff.linear_comment_posted = true;
        state.handoff.linear_comment_body_sha256 = Some(digest.clone());

        assert!(should_skip_handoff(&state, &digest));
        assert!(!should_skip_handoff(&state, &handoff_digest("different")));
    }

    #[tokio::test]
    async fn handoff_is_suppressed_when_disabled_in_settings() {
        let mut state = crate::state::RunState::for_start(
            "ENG-992",
            &crate::worktree::TargetWorktree {
                path: std::env::current_dir().unwrap(),
                branch: "feature/eng-992".to_string(),
                base_ref: "origin/main".to_string(),
            },
            true,
        )
        .unwrap();
        state.settings.linear_handoff_enabled = false;

        post_handoff_once(&mut state, "suppressed during safety test")
            .await
            .unwrap();

        assert!(!state.handoff.linear_comment_posted);
        assert!(state.handoff.linear_comment_body_sha256.is_none());
        assert!(state.handoff.posted_at.is_none());
    }
}
