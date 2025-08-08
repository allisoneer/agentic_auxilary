mod manager;
mod types;
mod validation;
// Add after line 2 (after existing modules)
mod mount_merger;
mod personal_manager;
mod repo_manager;
pub mod repo_mapping_manager;

pub use manager::ConfigManager;
pub use types::*;
pub use validation::MountValidator;
// Add after line 10 (after existing exports)
pub use mount_merger::{MountMerger, MountSource};
pub use personal_manager::PersonalConfigManager;
pub use repo_manager::RepoConfigManager;
pub use repo_mapping_manager::RepoMappingManager;
pub use types::{
    FileMetadata, MountDirs, MountPattern, PersonalConfig, PersonalMount, RepoConfig,
    RequiredMount, Rule,
};
// They are tested via their module unit tests
