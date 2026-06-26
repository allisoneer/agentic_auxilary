use crate::state::RunState;
use anyhow::Result;
use linear_tools::LinearTools;
use sha2::Digest;
use sha2::Sha256;

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
