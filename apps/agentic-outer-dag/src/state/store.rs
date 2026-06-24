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
