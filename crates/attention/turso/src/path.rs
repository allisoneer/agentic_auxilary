use fs2::FileExt;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use thiserror::Error;

const LOCK_FILE: &str = ".attention-turso.lock";
const DATABASE_FILE: &str = "attention.db";

#[derive(Debug, Error)]
pub enum PathError {
    #[error("storage path must be absolute")]
    NotAbsolute,
    #[error("storage path contains traversal or a non-normal component")]
    Traversal,
    #[error("storage path contains a symbolic-link component")]
    Symlink,
    #[error("storage path is not a directory")]
    NotDirectory,
    #[error("storage path is not valid UTF-8 for the pinned Turso API")]
    NonUtf8,
    #[error("storage directory permissions or ownership are unsafe")]
    UnsafePermissions,
    #[error("database and backup paths overlap")]
    Overlap,
    #[error("reader count {0} is outside 1..=16")]
    ReaderCount(usize),
    #[error("database directory is already owned by another process")]
    AlreadyOwned,
    #[error("storage path operation failed")]
    Io(#[source] io::Error),
}

impl From<io::Error> for PathError {
    fn from(source: io::Error) -> Self {
        Self::Io(source)
    }
}

#[derive(Debug, Clone)]
pub struct DatabaseDirectory(PathBuf);

impl DatabaseDirectory {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, PathError> {
        Ok(Self(validate_directory(path.as_ref())?))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn database_file(&self) -> PathBuf {
        self.0.join(DATABASE_FILE)
    }

    pub(crate) fn acquire(&self) -> Result<DirectoryOwnership, PathError> {
        DirectoryOwnership::acquire(self)
    }
}

#[derive(Debug, Clone)]
pub struct BackupRoot(PathBuf);

impl BackupRoot {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, PathError> {
        Ok(Self(validate_directory(path.as_ref())?))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

#[derive(Debug)]
pub struct DirectoryOwnership {
    file: File,
}

impl DirectoryOwnership {
    fn acquire(directory: &DatabaseDirectory) -> Result<Self, PathError> {
        let path = directory.as_path().join(LOCK_FILE);
        reject_symlink_components(&path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock_exclusive().map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                PathError::AlreadyOwned
            } else {
                PathError::Io(error)
            }
        })?;
        Ok(Self { file })
    }
}

impl Drop for DirectoryOwnership {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn validate_directory(path: &Path) -> Result<PathBuf, PathError> {
    validate_lexical(path)?;
    reject_symlink_components(path)?;
    create_secure_directory(path)?;
    let canonical = path.canonicalize()?;
    reject_symlink_components(&canonical)?;
    validate_directory_metadata(&canonical)?;
    Ok(canonical)
}

fn validate_lexical(path: &Path) -> Result<(), PathError> {
    if !path.is_absolute() {
        return Err(PathError::NotAbsolute);
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::CurDir | Component::Prefix(_)
        )
    }) {
        return Err(PathError::Traversal);
    }
    Ok(())
}

fn reject_symlink_components(path: &Path) -> Result<(), PathError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Err(PathError::Symlink),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(PathError::Io(error)),
        }
    }
    Ok(())
}

fn create_secure_directory(path: &Path) -> Result<(), PathError> {
    if path.exists() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700).create(path)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path)?;
    Ok(())
}

pub fn validate_directory_metadata(path: &Path) -> Result<(), PathError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(PathError::NotDirectory);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 || metadata.uid() != rustix::process::getuid().as_raw() {
            return Err(PathError::UnsafePermissions);
        }
    }
    Ok(())
}
