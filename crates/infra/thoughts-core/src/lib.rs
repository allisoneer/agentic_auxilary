pub mod config;
pub mod documents;
pub mod error;
pub mod fmt;
pub mod git;
pub mod mcp;
pub mod mount;
pub mod platform;
pub mod repo_identity;
pub mod utils;
pub mod workspace;

pub use config::{Config, Mount, SyncStrategy};
pub use config::{
    FileMetadata, MountDirs, RepoConfig, RepoConfigManager, RepoMappingManager, RequiredMount, Rule,
};
pub use documents::{
    ActiveDocuments, DocumentInfo, DocumentType, WriteDocumentOk, active_logs_dir, list_documents,
    write_document,
};
pub use error::{Result, ThoughtsError};
pub use mount::{MountInfo, MountOptions, MountSpace, get_mount_manager};
pub use platform::{Platform, PlatformInfo, detect_platform};
