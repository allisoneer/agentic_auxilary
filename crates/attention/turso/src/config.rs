use crate::path::BackupRoot;
use crate::path::DatabaseDirectory;
use crate::path::PathError;
use std::path::Path;
use std::time::Duration;

/// Measured bounded-reader default for the local adapter.
pub const DEFAULT_READER_COUNT: usize = 4;

/// Closed set of supported busy timeouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyTimeout {
    Interactive,
    Standard,
    Bulk,
}

impl BusyTimeout {
    pub const fn duration(self) -> Duration {
        match self {
            Self::Interactive => Duration::from_millis(50),
            Self::Standard => Duration::from_millis(250),
            Self::Bulk => Duration::from_secs(1),
        }
    }
}

/// Validated local adapter configuration.
#[derive(Debug, Clone)]
pub struct Config {
    database_directory: DatabaseDirectory,
    backup_root: BackupRoot,
    reader_count: usize,
    busy_timeout: BusyTimeout,
}

impl Config {
    /// Validate dedicated database and backup directories.
    pub fn new(
        database_directory: impl AsRef<Path>,
        backup_root: impl AsRef<Path>,
    ) -> Result<Self, PathError> {
        let database_directory = DatabaseDirectory::new(database_directory)?;
        let backup_root = BackupRoot::new(backup_root)?;
        if paths_overlap(database_directory.as_path(), backup_root.as_path()) {
            return Err(PathError::Overlap);
        }
        Ok(Self {
            database_directory,
            backup_root,
            reader_count: DEFAULT_READER_COUNT,
            busy_timeout: BusyTimeout::Standard,
        })
    }

    pub fn with_reader_count(mut self, reader_count: usize) -> Result<Self, PathError> {
        if !(1..=16).contains(&reader_count) {
            return Err(PathError::ReaderCount(reader_count));
        }
        self.reader_count = reader_count;
        Ok(self)
    }

    #[must_use]
    pub const fn with_busy_timeout(mut self, busy_timeout: BusyTimeout) -> Self {
        self.busy_timeout = busy_timeout;
        self
    }

    pub const fn database_directory(&self) -> &DatabaseDirectory {
        &self.database_directory
    }

    pub const fn backup_root(&self) -> &BackupRoot {
        &self.backup_root
    }

    pub const fn reader_count(&self) -> usize {
        self.reader_count
    }

    pub const fn busy_timeout(&self) -> BusyTimeout {
        self.busy_timeout
    }
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}
