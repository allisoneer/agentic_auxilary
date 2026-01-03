//! Just recipe discovery and execution module.
//!
//! Provides MCP tools for searching and executing justfile recipes across a repository.

pub mod cache;
pub mod discovery;
pub mod exec;
pub mod pager;
pub mod parser;
pub mod security;
pub mod types;

pub use cache::JustRegistry;
pub use types::{ExecuteOutput, ExecuteParams, SearchItem, SearchOutput, SearchParams};

use once_cell::sync::OnceCell;

static JUST_OK: OnceCell<bool> = OnceCell::new();

/// Ensure the `just` binary is available and functional.
pub async fn ensure_just_available() -> Result<(), String> {
    if JUST_OK.get().copied() == Some(true) {
        return Ok(());
    }
    let out = tokio::process::Command::new("just")
        .arg("--version")
        .output()
        .await
        .map_err(|e| format!("Failed to run 'just --version': {e}. Is 'just' installed?"))?;
    if !out.status.success() {
        return Err("`just` CLI returned non-zero; please install/repair".into());
    }
    JUST_OK.set(true).ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn just_binary_check() {
        // This test will pass if just is installed, skip gracefully if not
        let result = ensure_just_available().await;
        if result.is_err() {
            eprintln!("Skipping test: just binary not available");
            return;
        }
        // Second call should use cached result
        assert!(ensure_just_available().await.is_ok());
    }
}
