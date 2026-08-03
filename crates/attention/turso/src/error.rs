use crate::path::PathError;
use std::error::Error as StdError;
use std::io;
use thiserror::Error;

type BoxError = Box<dyn StdError + Send + Sync>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("storage configuration or path is invalid")]
    Path(#[from] PathError),
    #[error("database directory ownership failed")]
    Ownership(#[source] BoxError),
    #[error("database lifecycle does not permit this operation")]
    Lifecycle,
    #[error("database is shutting down")]
    Shutdown,
    #[error("database open failed")]
    Open(#[source] BoxError),
    #[error("database connection failed")]
    Connect(#[source] BoxError),
    #[error("database is busy")]
    Busy(#[source] BoxError),
    #[error("database snapshot is stale")]
    BusySnapshot(#[source] BoxError),
    #[error("database constraint failed")]
    Constraint(#[source] BoxError),
    #[error("database I/O failed")]
    Io(#[source] io::Error),
    #[error("database storage is full")]
    Full(#[source] BoxError),
    #[error("database is read-only")]
    ReadOnly(#[source] BoxError),
    #[error("database is corrupt or is not a database")]
    Corrupt(#[source] BoxError),
    #[error("database value decode failed")]
    Decode(#[source] BoxError),
    #[error("persisted {record} codec version {version} is unsupported ({byte_length} bytes)")]
    UnsupportedCodec {
        record: &'static str,
        version: i64,
        byte_length: usize,
    },
    #[error("persisted {record} codec version {version} is malformed ({byte_length} bytes)")]
    MalformedCodec {
        record: &'static str,
        version: i64,
        byte_length: usize,
    },
    #[error("migration integrity check failed: {0}")]
    MigrationIntegrity(&'static str),
    #[error("backup or restore failed: {0}")]
    Backup(&'static str),
    #[error("backup or restore I/O failed")]
    BackupIo(#[source] io::Error),
    #[error("commit outcome is unknown")]
    CommitOutcomeUnknown,
    #[error("qualification probe identity has a conflicting fingerprint or value")]
    ProbeIdentityConflict,
    #[error("database engine operation failed")]
    Engine(#[source] BoxError),
}

impl Error {
    pub(crate) fn from_open(source: turso_db::Error) -> Self {
        Self::Open(Box::new(source))
    }

    pub(crate) fn from_connect(source: turso_db::Error) -> Self {
        Self::Connect(Box::new(source))
    }
}

impl From<turso_db::Error> for Error {
    fn from(source: turso_db::Error) -> Self {
        match source {
            error @ turso_db::Error::Busy(_) => Self::Busy(Box::new(error)),
            error @ turso_db::Error::BusySnapshot(_) => Self::BusySnapshot(Box::new(error)),
            error @ turso_db::Error::Constraint(_) => Self::Constraint(Box::new(error)),
            error @ turso_db::Error::DatabaseFull(_) => Self::Full(Box::new(error)),
            error @ turso_db::Error::Readonly(_) => Self::ReadOnly(Box::new(error)),
            error @ (turso_db::Error::Corrupt(_) | turso_db::Error::NotAdb(_)) => {
                Self::Corrupt(Box::new(error))
            }
            turso_db::Error::IoError(kind, context) => Self::Io(io::Error::new(kind, context)),
            error => Self::Engine(Box::new(error)),
        }
    }
}
