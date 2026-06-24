use crate::state::RunState;
use anyhow::Result;
use linear_tools::LinearTools;
use sha2::Digest;
use sha2::Sha256;

pub async fn post_handoff_once(state: &mut RunState, message: &str) -> Result<()> {
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
}
