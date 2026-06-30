use crate::state::RunState;
use crate::state::STATE_FILENAME;
use anyhow::Context;
use anyhow::Result;
use std::fs;
use thoughts_tool::DocumentType;

pub struct ThoughtsStateStore;

impl ThoughtsStateStore {
    pub fn load() -> Result<Option<RunState>> {
        let active = thoughts_tool::workspace::ensure_active_work().context(
            "failed to resolve active thoughts workspace; chdir into a feature worktree first",
        )?;
        let path = active.artifacts.join(STATE_FILENAME);
        if !path.exists() {
            return Ok(None);
        }

        let json = fs::read_to_string(&path)
            .with_context(|| format!("failed to read state file at {}", path.display()))?;
        let state = serde_json::from_str(&json)
            .with_context(|| format!("failed to deserialize state file at {}", path.display()))?;
        validate_schema_version(&state, &path)?;
        Ok(Some(state))
    }

    pub fn save(state: &RunState) -> Result<()> {
        let mut state = state.clone();
        state.touch();
        let json = serde_json::to_string_pretty(&state)?;
        thoughts_tool::write_document(&DocumentType::Artifact, STATE_FILENAME, &json)
            .context("failed to persist branch-scoped outer DAG state")?;
        Ok(())
    }

    pub fn delete() -> Result<()> {
        let active = thoughts_tool::workspace::ensure_active_work().context(
            "failed to resolve active thoughts workspace; chdir into a feature worktree first",
        )?;
        let path = active.artifacts.join(STATE_FILENAME);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to remove state file at {}", path.display()))?;
        }
        Ok(())
    }
}

fn validate_schema_version(state: &RunState, path: &std::path::Path) -> Result<()> {
    anyhow::ensure!(
        state.schema_version == crate::state::SCHEMA_VERSION,
        "outer DAG state schema_version mismatch at {}: expected {}, found {}. Run `agentic-outer-dag reset --yes` to delete the stale state file.",
        path.display(),
        crate::state::SCHEMA_VERSION,
        state.schema_version
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_schema_version;
    use crate::state;
    use crate::worktree::TargetWorktree;

    fn sample_state() -> state::RunState {
        state::RunState::for_start(
            "ENG-992",
            &TargetWorktree {
                path: std::env::current_dir().unwrap(),
                branch: "feature/eng-992".to_string(),
                base_ref: "origin/main".to_string(),
            },
            false,
        )
        .unwrap()
    }

    #[test]
    fn validate_schema_version_accepts_current_version() {
        let state = sample_state();
        validate_schema_version(&state, std::path::Path::new("state.json")).unwrap();
    }

    #[test]
    fn validate_schema_version_rejects_mismatch_with_reset_guidance() {
        let mut state = sample_state();
        state.schema_version = state.schema_version.saturating_add(1);

        let err = validate_schema_version(&state, std::path::Path::new("state.json"))
            .expect_err("schema mismatch should fail");
        let text = err.to_string();
        assert!(text.contains("state.json"));
        assert!(text.contains("schema_version mismatch"));
        assert!(text.contains("reset --yes"));
    }
}
