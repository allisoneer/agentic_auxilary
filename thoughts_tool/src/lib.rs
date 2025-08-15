pub mod config;
pub mod error;
pub mod git;
pub mod mount;
pub mod platform;
pub mod utils;

pub use config::{Config, Mount, MountType, SyncStrategy};
// Add after line 10 (after existing config exports)
pub use config::{
    FileMetadata, MountDirs, MountPattern, PersonalConfig, PersonalConfigManager, PersonalMount,
    RepoConfig, RepoConfigManager, RepoMappingManager, RequiredMount, Rule,
    MountMerger, MountSource,
};
pub use error::{Result, ThoughtsError};
#[cfg(test)]
pub use mount::MockMountManager;
pub use mount::{MountInfo, MountOptions, get_mount_manager};
pub use platform::{Platform, PlatformInfo, detect_platform};
