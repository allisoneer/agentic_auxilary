mod manager;
mod types;
mod validation;
// Add after line 2 (after existing modules)
mod repo_manager;
mod personal_manager;
pub mod repo_mapping_manager;
mod mount_merger;

pub use manager::ConfigManager;
pub use types::*;
pub use validation::MountValidator;
// Add after line 10 (after existing exports)
pub use repo_manager::RepoConfigManager;
pub use personal_manager::PersonalConfigManager;
pub use repo_mapping_manager::RepoMappingManager;
pub use mount_merger::{MountMerger, MountSource};
pub use types::{RepoConfig, PersonalConfig, RequiredMount, MountPattern, PersonalMount, Rule, MountDirs, FileMetadata};
// They are tested via their module unit tests
