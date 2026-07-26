use fs2::FileExt;
use std::fs;
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
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

#[derive(Debug)]
struct ValidatedDirectory {
    path: PathBuf,
    descriptor: Arc<File>,
}

#[derive(Debug, Clone)]
pub struct DatabaseDirectory(Arc<ValidatedDirectory>);

impl DatabaseDirectory {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, PathError> {
        Ok(Self(Arc::new(validate_directory(path.as_ref())?)))
    }

    pub fn as_path(&self) -> &Path {
        &self.0.path
    }

    pub fn database_file(&self) -> PathBuf {
        self.0.path.join(DATABASE_FILE)
    }

    pub(crate) fn acquire(&self) -> Result<DirectoryOwnership, PathError> {
        DirectoryOwnership::acquire(self)
    }
}

#[derive(Debug, Clone)]
pub struct BackupRoot(Arc<ValidatedDirectory>);

impl BackupRoot {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, PathError> {
        Ok(Self(Arc::new(validate_directory(path.as_ref())?)))
    }

    pub fn as_path(&self) -> &Path {
        &self.0.path
    }
}

#[derive(Debug)]
pub struct DirectoryOwnership {
    file: File,
}

impl DirectoryOwnership {
    fn acquire(directory: &DatabaseDirectory) -> Result<Self, PathError> {
        let descriptor = rustix::fs::openat(
            &directory.0.descriptor,
            LOCK_FILE,
            rustix::fs::OFlags::RDWR
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::from_bits_truncate(0o600),
        )
        .map_err(map_lock_open_error)?;
        let file = File::from(descriptor);
        validate_lock_descriptor(&file)?;
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

fn validate_directory(path: &Path) -> Result<ValidatedDirectory, PathError> {
    validate_path_encoding(path)?;
    let normalized = normalize_absolute(path)?;
    let descriptor = walk_directory(&normalized)?;
    validate_storage_directory_descriptor(&descriptor)?;
    Ok(ValidatedDirectory {
        path: normalized,
        descriptor: Arc::new(descriptor),
    })
}

pub fn validate_path_encoding(path: &Path) -> Result<(), PathError> {
    if path.to_str().is_none() {
        return Err(PathError::NonUtf8);
    }
    Ok(())
}

fn normalize_absolute(path: &Path) -> Result<PathBuf, PathError> {
    if !path.is_absolute() {
        return Err(PathError::NotAbsolute);
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Normal(_) => normalized.push(component.as_os_str()),
            Component::ParentDir | Component::CurDir | Component::Prefix(_) => {
                return Err(PathError::Traversal);
            }
        }
    }
    Ok(normalized)
}

fn walk_directory(path: &Path) -> Result<File, PathError> {
    let root = rustix::fs::open("/", directory_open_flags(), rustix::fs::Mode::empty())
        .map(File::from)
        .map_err(|error| PathError::Io(io::Error::from(error)))?;
    let mut current = root;
    for component in path.components() {
        if let Component::Normal(name) = component {
            current = open_or_create_directory(&current, name)?;
        }
    }
    Ok(current)
}

fn directory_open_flags() -> rustix::fs::OFlags {
    rustix::fs::OFlags::RDONLY
        | rustix::fs::OFlags::DIRECTORY
        | rustix::fs::OFlags::NOFOLLOW
        | rustix::fs::OFlags::CLOEXEC
}

fn open_or_create_directory(parent: &File, name: &std::ffi::OsStr) -> Result<File, PathError> {
    open_or_create_directory_after_missing(parent, name, || Ok(()))
}

fn open_or_create_directory_after_missing(
    parent: &File,
    name: &std::ffi::OsStr,
    after_missing: impl FnOnce() -> Result<(), PathError>,
) -> Result<File, PathError> {
    match rustix::fs::openat(
        parent,
        name,
        directory_open_flags(),
        rustix::fs::Mode::empty(),
    ) {
        Ok(descriptor) => Ok(File::from(descriptor)),
        Err(rustix::io::Errno::NOENT) => {
            after_missing()?;
            match rustix::fs::mkdirat(parent, name, rustix::fs::Mode::from_bits_truncate(0o700)) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(error) => return Err(PathError::Io(io::Error::from(error))),
            }
            rustix::fs::openat(
                parent,
                name,
                directory_open_flags(),
                rustix::fs::Mode::empty(),
            )
            .map(File::from)
            .map_err(|error| map_directory_open_error(parent, name, error))
        }
        Err(error) => Err(map_directory_open_error(parent, name, error)),
    }
}

fn map_directory_open_error(
    parent: &File,
    name: &std::ffi::OsStr,
    error: rustix::io::Errno,
) -> PathError {
    if error == rustix::io::Errno::LOOP {
        return PathError::Symlink;
    }
    if error != rustix::io::Errno::NOTDIR {
        return PathError::Io(io::Error::from(error));
    }
    match rustix::fs::openat(
        parent,
        name,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    ) {
        Err(rustix::io::Errno::LOOP) => PathError::Symlink,
        Ok(_) | Err(_) => PathError::NotDirectory,
    }
}

fn map_lock_open_error(error: rustix::io::Errno) -> PathError {
    if error == rustix::io::Errno::LOOP {
        PathError::Symlink
    } else {
        PathError::Io(io::Error::from(error))
    }
}

fn validate_storage_directory_descriptor(directory: &File) -> Result<(), PathError> {
    let metadata =
        rustix::fs::fstat(directory).map_err(|error| PathError::Io(io::Error::from(error)))?;
    if rustix::fs::FileType::from_raw_mode(metadata.st_mode) != rustix::fs::FileType::Directory {
        return Err(PathError::NotDirectory);
    }
    if metadata.st_mode & 0o077 != 0 || metadata.st_uid != rustix::process::getuid().as_raw() {
        return Err(PathError::UnsafePermissions);
    }
    Ok(())
}

fn validate_lock_descriptor(lock: &File) -> Result<(), PathError> {
    let metadata =
        rustix::fs::fstat(lock).map_err(|error| PathError::Io(io::Error::from(error)))?;
    if rustix::fs::FileType::from_raw_mode(metadata.st_mode) != rustix::fs::FileType::RegularFile {
        return Err(PathError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "ownership lock is not a regular file",
        )));
    }
    if metadata.st_uid != rustix::process::getuid().as_raw() {
        return Err(PathError::UnsafePermissions);
    }
    Ok(())
}

pub fn validate_directory_metadata(path: &Path) -> Result<(), PathError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(PathError::NotDirectory);
    }
    if metadata.mode() & 0o077 != 0 || metadata.uid() != rustix::process::getuid().as_raw() {
        return Err(PathError::UnsafePermissions);
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    #[test]
    fn rustix_relative_directory_slice_creates_reopens_and_stats() -> TestResult {
        let root = tempfile::tempdir()?;
        let parent = rustix::fs::open(
            root.path(),
            directory_open_flags(),
            rustix::fs::Mode::empty(),
        )
        .map(File::from)?;

        let child = open_or_create_directory(&parent, OsStr::new("child"))?;
        let metadata = rustix::fs::fstat(&child)?;
        assert_eq!(
            rustix::fs::FileType::from_raw_mode(metadata.st_mode),
            rustix::fs::FileType::Directory
        );
        assert_eq!(metadata.st_mode & 0o077, 0);

        let reopened = open_or_create_directory(&parent, OsStr::new("child"))?;
        let reopened_metadata = rustix::fs::fstat(&reopened)?;
        assert_eq!(metadata.st_dev, reopened_metadata.st_dev);
        assert_eq!(metadata.st_ino, reopened_metadata.st_ino);
        Ok(())
    }

    #[test]
    fn lexical_normalization_preserves_absolute_normal_components() -> TestResult {
        let normalized = normalize_absolute(Path::new("/one//two"))?;
        assert_eq!(normalized, Path::new("/one/two"));
        assert!(matches!(
            normalize_absolute(Path::new("relative")),
            Err(PathError::NotAbsolute)
        ));
        assert!(matches!(
            normalize_absolute(Path::new("/one/../two")),
            Err(PathError::Traversal)
        ));
        Ok(())
    }

    #[test]
    fn concurrent_creator_is_reopened_and_validated() -> TestResult {
        let root = tempfile::tempdir()?;
        let parent = rustix::fs::open(
            root.path(),
            directory_open_flags(),
            rustix::fs::Mode::empty(),
        )
        .map(File::from)?;
        let child_path = root.path().join("child");

        let child = open_or_create_directory_after_missing(&parent, OsStr::new("child"), || {
            fs::create_dir_all(&child_path)?;
            Ok(())
        })?;

        let metadata = rustix::fs::fstat(&child)?;
        assert_eq!(
            rustix::fs::FileType::from_raw_mode(metadata.st_mode),
            rustix::fs::FileType::Directory
        );
        Ok(())
    }

    #[test]
    fn rejects_non_directory_component_and_unsafe_final_mode() -> TestResult {
        let root = tempfile::tempdir()?;
        let file_path = root.path().join("file");
        File::create(&file_path)?;
        assert!(matches!(
            DatabaseDirectory::new(&file_path),
            Err(PathError::NotDirectory)
        ));
        assert!(matches!(
            DatabaseDirectory::new(file_path.join("child")),
            Err(PathError::NotDirectory)
        ));

        let symlink_path = root.path().join("symlink");
        symlink(root.path(), &symlink_path)?;
        assert!(matches!(
            DatabaseDirectory::new(symlink_path),
            Err(PathError::Symlink)
        ));

        let unsafe_path = root.path().join("unsafe");
        fs::create_dir_all(&unsafe_path)?;
        fs::set_permissions(&unsafe_path, fs::Permissions::from_mode(0o750))?;
        assert!(matches!(
            DatabaseDirectory::new(unsafe_path),
            Err(PathError::UnsafePermissions)
        ));
        Ok(())
    }

    #[test]
    fn retained_parent_descriptor_targets_renamed_inode() -> TestResult {
        let root = tempfile::tempdir()?;
        let original = root.path().join("original");
        let renamed = root.path().join("renamed");
        fs::create_dir_all(&original)?;
        let parent = rustix::fs::open(&original, directory_open_flags(), rustix::fs::Mode::empty())
            .map(File::from)?;

        fs::rename(&original, &renamed)?;
        fs::create_dir_all(&original)?;
        let child = open_or_create_directory(&parent, OsStr::new("child"))?;

        assert!(!original.join("child").exists());
        let child_metadata = rustix::fs::fstat(&child)?;
        let renamed_child_metadata = fs::metadata(renamed.join("child"))?;
        assert_eq!(child_metadata.st_dev, renamed_child_metadata.dev());
        assert_eq!(child_metadata.st_ino, renamed_child_metadata.ino());
        assert_eq!(child_metadata.st_mode & 0o077, 0);
        Ok(())
    }

    #[test]
    fn opened_non_regular_lock_descriptor_is_rejected() -> TestResult {
        let root = tempfile::tempdir()?;
        let directory = File::open(root.path())?;
        assert!(matches!(
            validate_lock_descriptor(&directory),
            Err(PathError::Io(_))
        ));
        Ok(())
    }
}
