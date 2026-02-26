mod repo_manager;
pub mod repo_mapping_manager;
mod types;
pub mod validation;

pub use repo_manager::RepoConfigManager;
pub use repo_mapping_manager::{RepoMappingManager, extract_org_repo_from_url};
pub use types::*;
// They are tested via their module unit tests

// Re-export agentic-config types for convenience
pub use agentic_config::types::{
    ContextMount as AgenticContextMount, ReferenceEntry as AgenticReferenceEntry,
    ReferenceMount as AgenticReferenceMount, SyncStrategy as AgenticSyncStrategy,
    ThoughtsConfig as AgenticThoughtsConfig, ThoughtsMount as AgenticThoughtsMount,
    ThoughtsMountDirs as AgenticThoughtsMountDirs,
};

/// Load thoughts configuration via the unified agentic-config system.
///
/// This function provides an alternative path for loading configuration
/// that uses the new `agentic.json` format with automatic migration from
/// legacy `.thoughts/config.json` V2 files.
///
/// # Returns
/// - `Ok(loaded)` with the merged configuration, warnings, and events
/// - Events include `MigratedThoughtsV2` if migration occurred
///
/// # Example
/// ```no_run
/// use thoughts_tool::config::load_agentic_config;
///
/// let repo_root = std::env::current_dir().unwrap();
/// let loaded = load_agentic_config(&repo_root).unwrap();
///
/// for event in &loaded.events {
///     eprintln!("Config event: {:?}", event);
/// }
///
/// let thoughts = &loaded.config.thoughts;
/// println!("Mount dirs: {:?}", thoughts.mount_dirs);
/// ```
pub fn load_agentic_config(
    repo_root: &std::path::Path,
) -> anyhow::Result<agentic_config::LoadedAgenticConfig> {
    agentic_config::load_merged(repo_root)
}
