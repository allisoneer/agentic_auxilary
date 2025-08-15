pub mod config;
pub mod error;
pub mod git;
pub mod mount;
pub mod platform;
pub mod utils;

pub use config::{Config, Mount, SyncStrategy};
// Add after line 10 (after existing config exports)
pub use config::{
    FileMetadata, MountDirs, MountMerger, MountPattern, MountSource, PersonalConfig,
    PersonalConfigManager, PersonalMount, RepoConfig, RepoConfigManager, RepoMappingManager,
    RequiredMount, Rule,
};
pub use error::{Result, ThoughtsError};
pub use mount::{MountInfo, MountOptions, get_mount_manager};
pub use platform::{Platform, PlatformInfo, detect_platform};
