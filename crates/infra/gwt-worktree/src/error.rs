use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("gwt config directory is unavailable")]
    ConfigDirectoryUnavailable,
    #[error("git error: {0}")]
    Git(#[from] git2::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML deserialize error: {0}")]
    TomlDeserialize(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("could not resolve control repository")]
    ControlRepoNotFound,
    #[error("invalid branch name: {0}")]
    InvalidBranchName(String),
    #[error("invalid worktree admin name: {0}")]
    InvalidAdminName(String),
    #[error("invalid worktree admin encoding: {0}")]
    InvalidAdminEncoding(String),
    #[error("remote refresh capability is required")]
    MissingRemoteRefresher,
    #[error("branch not found: {0}")]
    BranchNotFound(String),
    #[error("missing switch start point for force-create")]
    MissingStartPoint,
    #[error("invalid object id: {0}")]
    InvalidObjectId(String),
    #[error("cannot remove the main worktree")]
    CannotRemoveMainWorktree,
    #[error("worktree has uncommitted changes")]
    DirtyWorktree,
    #[error("worktree is locked")]
    LockedWorktree,
    #[error("worktree is outside the canonical .gwt base")]
    WorktreeOutsideBase,
    #[error("remote branch deleter is required")]
    MissingRemoteBranchDeleter,
}
