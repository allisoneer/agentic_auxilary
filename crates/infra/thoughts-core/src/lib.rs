#[cfg(not(unix))]
compile_error!(
    "thoughts-tool only supports Unix-like platforms (Linux/macOS). Windows is not supported."
);

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

pub use config::Config;
pub use config::Mount;
pub use config::SyncStrategy;
// Note: V1 config types (RepoConfig, MountDirs, RequiredMount, FileMetadata, Rule, etc.)
// have been removed. Use V2 config APIs (load_desired_state, load_v2_or_bail, etc.).
pub use config::RepoConfigManager;
pub use config::RepoMappingManager;
pub use documents::ActiveDocuments;
pub use documents::DocumentInfo;
pub use documents::DocumentType;
pub use documents::WriteDocumentOk;
pub use documents::active_logs_dir;
pub use documents::list_documents;
pub use documents::write_document;
pub use error::Result;
pub use error::ThoughtsError;
pub use mount::MountInfo;
pub use mount::MountOptions;
pub use mount::MountSpace;
pub use mount::get_mount_manager;
pub use platform::Platform;
pub use platform::PlatformInfo;
pub use platform::detect_platform;
