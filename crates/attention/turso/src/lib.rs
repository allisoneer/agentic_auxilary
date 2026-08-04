//! Native local Turso storage adapter for Attention.
//!
//! Turso engine types and SQL remain private to this crate. The adapter implements
//! the kernel read and commit ports while preserving the T06/T08/T09 boundaries.

#[cfg(not(unix))]
compile_error!(
    "attention-turso requires a Unix target; qualification coverage is limited to Linux and macOS"
);

mod backup;
mod codec;
mod config;
mod database;
mod decode;
mod domain_sql;
mod error;
mod lifecycle;
mod mapping;
mod migration;
mod path;
mod reader;
mod semantic_reader;
mod semantic_writer;
mod sql;
mod store;
mod writer;

pub use backup::BackupEntry;
pub use backup::BackupManifest;
pub use config::BusyTimeout;
pub use config::Config;
pub use config::DEFAULT_READER_COUNT;
pub use database::AttentionDatabase;
pub use database::ProbeResolution;
pub use error::Error;
pub use lifecycle::LifecycleState;
pub use migration::MIGRATION_HEAD;
pub use migration::MigrationReport;
pub use path::BackupRoot;
pub use path::DatabaseDirectory;
pub use path::PathError;
pub use writer::CommitPhase;
pub use writer::ProbeWriteOutcome;

/// Exact upstream Turso package version qualified by this adapter.
pub const PINNED_TURSO_VERSION: &str = "0.8.0-pre.1";

/// Operational WAL/file-set threshold selected from retained qualification workloads.
pub const WAL_OPERATIONAL_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
