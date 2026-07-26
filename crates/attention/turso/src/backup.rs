use crate::Config;
use crate::Error;
use crate::MIGRATION_HEAD;
use crate::PINNED_TURSO_VERSION;
use crate::migration::MIGRATIONS;
use crate::path::validate_directory_metadata;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
use std::path::Component;
use std::path::Path;

const FORMAT_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "manifest.json";
const LOCK_FILE: &str = ".attention-turso.lock";
const BACKUP_STAGING_CLEANUP_INCOMPLETE: &str =
    "backup failed and staging directory could not be cleaned up; manual removal required";
const RESTORE_RECOVERY_INCOMPLETE: &str =
    "restore recovery is incomplete; database and staging directories were preserved";

trait FileOperations {
    fn rename(&mut self, source: &Path, destination: &Path) -> std::io::Result<()>;
    fn remove_dir(&mut self, path: &Path) -> std::io::Result<()>;
    fn remove_dir_all(&mut self, path: &Path) -> std::io::Result<()>;
    fn sync_directory(&mut self, path: &Path) -> std::io::Result<()>;
}

struct RealFileOperations;

impl FileOperations for RealFileOperations {
    fn rename(&mut self, source: &Path, destination: &Path) -> std::io::Result<()> {
        fs::rename(source, destination)
    }

    fn remove_dir(&mut self, path: &Path) -> std::io::Result<()> {
        fs::remove_dir(path)
    }

    fn remove_dir_all(&mut self, path: &Path) -> std::io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn sync_directory(&mut self, path: &Path) -> std::io::Result<()> {
        sync_directory(path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupEntry {
    path: String,
    size: u64,
    checksum: Vec<u8>,
}

impl BackupEntry {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn size(&self) -> u64 {
        self.size
    }

    pub fn checksum(&self) -> &[u8] {
        &self.checksum
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupManifest {
    format_version: u32,
    adapter_version: String,
    turso_version: String,
    migration_head: u64,
    migration_checksum: Vec<u8>,
    files: Vec<BackupEntry>,
}

impl BackupManifest {
    pub const fn format_version(&self) -> u32 {
        self.format_version
    }

    pub fn files(&self) -> &[BackupEntry] {
        &self.files
    }
}

pub fn create(config: &Config, name: &str) -> Result<BackupManifest, Error> {
    create_with_operations(config, name, &mut RealFileOperations)
}

fn create_with_operations(
    config: &Config,
    name: &str,
    operations: &mut impl FileOperations,
) -> Result<BackupManifest, Error> {
    validate_name(name)?;
    let _ownership = config
        .database_directory()
        .acquire()
        .map_err(|error| Error::Ownership(Box::new(error)))?;
    let final_path = config.backup_root().as_path().join(name);
    if final_path.exists() {
        return Err(Error::Backup("completed backup destination already exists"));
    }
    let staging = config.backup_root().as_path().join(format!(
        ".attention-turso-stage-{}-{name}",
        std::process::id()
    ));
    if staging.exists() {
        return Err(Error::Backup("backup staging destination already exists"));
    }
    create_private_directory(&staging)?;
    let manifest = match create_staged(config.database_directory().as_path(), &staging) {
        Ok(manifest) => manifest,
        Err(error) => {
            if operations.remove_dir_all(&staging).is_err() {
                return Err(Error::Backup(BACKUP_STAGING_CLEANUP_INCOMPLETE));
            }
            return Err(error);
        }
    };
    if let Err(error) = operations.sync_directory(&staging) {
        if operations.remove_dir_all(&staging).is_err() {
            return Err(Error::Backup(BACKUP_STAGING_CLEANUP_INCOMPLETE));
        }
        return Err(Error::BackupIo(error));
    }
    if let Err(error) = operations.rename(&staging, &final_path) {
        if operations.remove_dir_all(&staging).is_err() {
            return Err(Error::Backup(BACKUP_STAGING_CLEANUP_INCOMPLETE));
        }
        return Err(Error::BackupIo(error));
    }
    operations
        .sync_directory(config.backup_root().as_path())
        .map_err(Error::BackupIo)?;
    Ok(manifest)
}

pub fn restore_files(config: &Config, name: &str) -> Result<BackupManifest, Error> {
    restore_files_with_operations(config, name, &mut RealFileOperations)
}

fn restore_files_with_operations(
    config: &Config,
    name: &str,
    operations: &mut impl FileOperations,
) -> Result<BackupManifest, Error> {
    validate_name(name)?;
    let ownership = config
        .database_directory()
        .acquire()
        .map_err(|error| Error::Ownership(Box::new(error)))?;
    ensure_empty_target(config.database_directory().as_path())?;
    let backup = config.backup_root().as_path().join(name);
    let metadata = fs::symlink_metadata(&backup).map_err(Error::BackupIo)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Error::Backup("backup is not a safe directory"));
    }
    validate_directory_metadata(&backup)?;
    let manifest = read_manifest(&backup)?;
    validate_manifest(&manifest)?;
    validate_inventory(&backup, &manifest)?;

    let staging = config
        .database_directory()
        .as_path()
        .join(".attention-turso-restore-stage");
    create_private_directory(&staging)?;
    let result = restore_staged(&backup, &staging, &manifest, operations);
    if let Err(error) = result {
        if operations.remove_dir_all(&staging).is_err() {
            return Err(Error::Backup(RESTORE_RECOVERY_INCOMPLETE));
        }
        return Err(error);
    }
    let mut moved = Vec::with_capacity(manifest.files.len());
    for entry in &manifest.files {
        let source = staging.join(&entry.path);
        let destination = config.database_directory().as_path().join(&entry.path);
        if let Err(error) = operations.rename(&source, &destination) {
            let mut rollback_complete = true;
            for path in moved.iter().rev() {
                if operations
                    .rename(
                        &config.database_directory().as_path().join(path),
                        &staging.join(path),
                    )
                    .is_err()
                {
                    rollback_complete = false;
                }
            }
            if !rollback_complete || operations.remove_dir_all(&staging).is_err() {
                return Err(Error::Backup(RESTORE_RECOVERY_INCOMPLETE));
            }
            return Err(Error::BackupIo(error));
        }
        moved.push(entry.path.as_str());
    }
    if operations.remove_dir(&staging).is_err() {
        return Err(Error::Backup(RESTORE_RECOVERY_INCOMPLETE));
    }
    operations
        .sync_directory(config.database_directory().as_path())
        .map_err(Error::BackupIo)?;
    drop(ownership);
    Ok(manifest)
}

fn create_staged(source: &Path, staging: &Path) -> Result<BackupManifest, Error> {
    let inventory = inventory(source, false)?;
    if inventory.is_empty() {
        return Err(Error::Backup("database file set is empty"));
    }
    let mut files = Vec::with_capacity(inventory.len());
    for relative in inventory {
        let source_file = source.join(&relative);
        let destination = staging.join(&relative);
        let (size, checksum) = copy_and_hash(&source_file, &destination)?;
        files.push(BackupEntry {
            path: relative,
            size,
            checksum,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let manifest = BackupManifest {
        format_version: FORMAT_VERSION,
        adapter_version: env!("CARGO_PKG_VERSION").to_string(),
        turso_version: PINNED_TURSO_VERSION.to_string(),
        migration_head: MIGRATION_HEAD,
        migration_checksum: migration_checksum(),
        files,
    };
    let manifest_path = staging.join(MANIFEST_FILE);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(manifest_path)
        .map_err(Error::BackupIo)?;
    serde_json::to_writer_pretty(&mut output, &manifest)
        .map_err(|_| Error::Backup("manifest serialization failed"))?;
    output.write_all(b"\n").map_err(Error::BackupIo)?;
    output.sync_all().map_err(Error::BackupIo)?;
    Ok(manifest)
}

fn restore_staged(
    backup: &Path,
    staging: &Path,
    manifest: &BackupManifest,
    operations: &mut impl FileOperations,
) -> Result<(), Error> {
    for entry in &manifest.files {
        let source = backup.join(&entry.path);
        let destination = staging.join(&entry.path);
        let (size, checksum) = copy_and_hash(&source, &destination)?;
        if size != entry.size || checksum != entry.checksum {
            return Err(Error::Backup("backup changed while restoring"));
        }
    }
    operations.sync_directory(staging).map_err(Error::BackupIo)
}

fn read_manifest(backup: &Path) -> Result<BackupManifest, Error> {
    let mut input = open_nofollow(&backup.join(MANIFEST_FILE))?;
    serde_json::from_reader(&mut input).map_err(|_| Error::Backup("manifest is malformed"))
}

fn validate_manifest(manifest: &BackupManifest) -> Result<(), Error> {
    if manifest.format_version != FORMAT_VERSION
        || manifest.adapter_version != env!("CARGO_PKG_VERSION")
        || manifest.turso_version != PINNED_TURSO_VERSION
        || manifest.migration_head != MIGRATION_HEAD
        || manifest.migration_checksum != migration_checksum()
    {
        return Err(Error::Backup("backup compatibility check failed"));
    }
    if manifest.files.is_empty() {
        return Err(Error::Backup("backup manifest contains no files"));
    }
    let mut previous = None;
    for entry in &manifest.files {
        validate_relative(&entry.path)?;
        if entry.checksum.len() != 32 {
            return Err(Error::Backup("backup checksum length is invalid"));
        }
        if previous.is_some_and(|value: &str| value >= entry.path.as_str()) {
            return Err(Error::Backup("backup inventory is duplicate or unsorted"));
        }
        previous = Some(entry.path.as_str());
    }
    Ok(())
}

fn validate_inventory(backup: &Path, manifest: &BackupManifest) -> Result<(), Error> {
    let actual = inventory(backup, true)?;
    let expected: Vec<_> = manifest
        .files
        .iter()
        .map(|entry| entry.path.clone())
        .collect();
    if actual != expected {
        return Err(Error::Backup("backup inventory does not match manifest"));
    }
    for entry in &manifest.files {
        let mut input = open_nofollow(&backup.join(&entry.path))?;
        let metadata = input.metadata().map_err(Error::BackupIo)?;
        let checksum = hash_reader(&mut input)?;
        if metadata.len() != entry.size || checksum != entry.checksum {
            return Err(Error::Backup("backup file checksum or size mismatch"));
        }
    }
    Ok(())
}

fn inventory(directory: &Path, include_manifest: bool) -> Result<Vec<String>, Error> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).map_err(Error::BackupIo)? {
        let entry = entry.map_err(Error::BackupIo)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == LOCK_FILE || (include_manifest && name == MANIFEST_FILE) {
            continue;
        }
        validate_relative(&name)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(Error::BackupIo)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(Error::Backup("file set contains an unexpected entry type"));
        }
        files.push(name);
    }
    files.sort_unstable();
    Ok(files)
}

fn ensure_empty_target(target: &Path) -> Result<(), Error> {
    for entry in fs::read_dir(target).map_err(Error::BackupIo)? {
        let entry = entry.map_err(Error::BackupIo)?;
        if entry.file_name() != LOCK_FILE {
            return Err(Error::Backup("restore target is not empty"));
        }
    }
    Ok(())
}

fn copy_and_hash(source: &Path, destination: &Path) -> Result<(u64, Vec<u8>), Error> {
    let mut input = open_nofollow(source)?;
    if !input.metadata().map_err(Error::BackupIo)?.is_file() {
        return Err(Error::Backup("backup source entry is not a regular file"));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(Error::BackupIo)?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = input.read(&mut buffer).map_err(Error::BackupIo)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read]).map_err(Error::BackupIo)?;
        hasher.update(&buffer[..read]);
        size += u64::try_from(read).map_err(|_| Error::Backup("file size overflow"))?;
    }
    output.sync_all().map_err(Error::BackupIo)?;
    Ok((size, hasher.finalize().to_vec()))
}

fn hash_reader(reader: &mut File) -> Result<Vec<u8>, Error> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = reader.read(&mut buffer).map_err(Error::BackupIo)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_vec())
}

fn migration_checksum() -> Vec<u8> {
    let mut hasher = Sha256::new();
    for migration in MIGRATIONS {
        hasher.update(migration.version().to_le_bytes());
        hasher.update(migration.name().as_bytes());
        hasher.update(migration.checksum());
    }
    hasher.finalize().to_vec()
}

fn validate_name(name: &str) -> Result<(), Error> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(Error::Backup("backup name is invalid"));
    }
    Ok(())
}

fn validate_relative(path: &str) -> Result<(), Error> {
    let mut components = Path::new(path).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(Error::Backup("backup inventory path is unsafe"));
    }
    if path == LOCK_FILE || path == MANIFEST_FILE {
        return Err(Error::Backup("backup inventory contains a reserved path"));
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path).map_err(Error::BackupIo)?;
    Ok(())
}

fn open_nofollow(path: &Path) -> Result<File, Error> {
    use rustix::fs::Mode;
    use rustix::fs::OFlags;
    let descriptor = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|error| Error::BackupIo(std::io::Error::from(error)))?;
    Ok(File::from(descriptor))
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path).and_then(|directory| directory.sync_all())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;
    use std::io;
    use std::path::PathBuf;

    type TestResult<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;

    #[derive(Default)]
    struct ScriptedOperations {
        fail_renames: Vec<usize>,
        fail_remove_dirs: Vec<usize>,
        fail_remove_dir_alls: Vec<usize>,
        fail_syncs: Vec<usize>,
        rename_calls: usize,
        remove_dir_calls: usize,
        remove_dir_all_calls: usize,
        sync_calls: usize,
        rename_sources: Vec<PathBuf>,
    }

    impl ScriptedOperations {
        fn injected_error() -> io::Error {
            io::Error::other("injected filesystem failure")
        }
    }

    impl FileOperations for ScriptedOperations {
        fn rename(&mut self, source: &Path, destination: &Path) -> io::Result<()> {
            self.rename_calls += 1;
            self.rename_sources.push(source.to_path_buf());
            if self.fail_renames.contains(&self.rename_calls) {
                return Err(Self::injected_error());
            }
            fs::rename(source, destination)
        }

        fn remove_dir(&mut self, path: &Path) -> io::Result<()> {
            self.remove_dir_calls += 1;
            if self.fail_remove_dirs.contains(&self.remove_dir_calls) {
                return Err(Self::injected_error());
            }
            fs::remove_dir(path)
        }

        fn remove_dir_all(&mut self, path: &Path) -> io::Result<()> {
            self.remove_dir_all_calls += 1;
            if self
                .fail_remove_dir_alls
                .contains(&self.remove_dir_all_calls)
            {
                return Err(Self::injected_error());
            }
            fs::remove_dir_all(path)
        }

        fn sync_directory(&mut self, path: &Path) -> io::Result<()> {
            self.sync_calls += 1;
            if self.fail_syncs.contains(&self.sync_calls) {
                return Err(Self::injected_error());
            }
            sync_directory(path)
        }
    }

    #[test]
    fn backup_cleans_staging_before_publication_and_preserves_published_backup() -> TestResult {
        let (root, config) = prepared_source(&["alpha"])?;
        fs::create_dir_all(config.database_directory().as_path().join("unexpected"))?;
        assert!(create(&config, "staged-copy").is_err());
        assert!(!backup_staging(&config, "staged-copy").exists());
        drop(root);

        let (_root, config) = prepared_source(&["alpha"])?;
        let mut operations = ScriptedOperations {
            fail_syncs: vec![1],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            create_with_operations(&config, "staging-sync", &mut operations),
            Err(Error::BackupIo(_))
        ));
        assert!(!backup_staging(&config, "staging-sync").exists());
        assert!(!config.backup_root().as_path().join("staging-sync").exists());

        let (_root, config) = prepared_source(&["alpha"])?;
        let mut operations = ScriptedOperations {
            fail_renames: vec![1],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            create_with_operations(&config, "rename", &mut operations),
            Err(Error::BackupIo(_))
        ));
        assert!(!backup_staging(&config, "rename").exists());
        assert!(!config.backup_root().as_path().join("rename").exists());

        let (_root, config) = prepared_source(&["alpha"])?;
        let mut operations = ScriptedOperations {
            fail_syncs: vec![2],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            create_with_operations(&config, "published", &mut operations),
            Err(Error::BackupIo(_))
        ));
        assert!(!backup_staging(&config, "published").exists());
        assert!(config.backup_root().as_path().join("published").is_dir());
        Ok(())
    }

    #[test]
    fn backup_cleanup_failure_preserves_staging_and_reports_recovery() -> TestResult {
        let (_root, config) = prepared_source(&["alpha"])?;
        fs::create_dir_all(config.database_directory().as_path().join("unexpected"))?;
        let mut operations = ScriptedOperations {
            fail_remove_dir_alls: vec![1],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            create_with_operations(&config, "staged-copy", &mut operations),
            Err(Error::Backup(BACKUP_STAGING_CLEANUP_INCOMPLETE))
        ));
        assert!(backup_staging(&config, "staged-copy").is_dir());
        assert!(!config.backup_root().as_path().join("staged-copy").exists());

        let (_root, config) = prepared_source(&["alpha"])?;
        let mut operations = ScriptedOperations {
            fail_remove_dir_alls: vec![1],
            fail_syncs: vec![1],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            create_with_operations(&config, "staging-sync", &mut operations),
            Err(Error::Backup(BACKUP_STAGING_CLEANUP_INCOMPLETE))
        ));
        assert!(backup_staging(&config, "staging-sync").is_dir());
        assert!(!config.backup_root().as_path().join("staging-sync").exists());

        let (_root, config) = prepared_source(&["alpha"])?;
        let mut operations = ScriptedOperations {
            fail_remove_dir_alls: vec![1],
            fail_renames: vec![1],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            create_with_operations(&config, "rename", &mut operations),
            Err(Error::Backup(BACKUP_STAGING_CLEANUP_INCOMPLETE))
        ));
        assert!(backup_staging(&config, "rename").is_dir());
        assert!(!config.backup_root().as_path().join("rename").exists());
        Ok(())
    }

    #[test]
    fn restore_forward_failure_rolls_back_in_reverse_and_allows_retry() -> TestResult {
        let (_root, config) = prepared_restore()?;
        let mut operations = ScriptedOperations {
            fail_renames: vec![3],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            restore_files_with_operations(&config, "complete", &mut operations),
            Err(Error::BackupIo(_))
        ));
        assert_eq!(
            file_names(&operations.rename_sources[3..]),
            ["beta", "alpha"]
        );
        assert!(!restore_staging(&config).exists());
        assert!(!config.database_directory().as_path().join("alpha").exists());
        assert!(!config.database_directory().as_path().join("beta").exists());

        let manifest = restore_files(&config, "complete")?;
        assert_eq!(manifest.files().len(), 3);
        assert_eq!(
            fs::read(config.database_directory().as_path().join("alpha"))?,
            b"alpha"
        );
        Ok(())
    }

    #[test]
    fn restore_incomplete_rollback_preserves_both_directories() -> TestResult {
        let (_root, config) = prepared_restore()?;
        let mut operations = ScriptedOperations {
            fail_renames: vec![3, 4],
            ..ScriptedOperations::default()
        };
        let error = restore_files_with_operations(&config, "complete", &mut operations);
        assert!(matches!(
            error,
            Err(Error::Backup(RESTORE_RECOVERY_INCOMPLETE))
        ));
        assert_eq!(
            file_names(&operations.rename_sources[3..]),
            ["beta", "alpha"]
        );
        assert_eq!(operations.remove_dir_all_calls, 0);
        assert!(config.database_directory().as_path().join("beta").is_file());
        let staging = restore_staging(&config);
        assert!(staging.is_dir());
        assert!(staging.join("alpha").is_file());
        assert!(staging.join("gamma").is_file());
        Ok(())
    }

    #[test]
    fn restore_staging_failure_cleanup_allows_retry() -> TestResult {
        let (_root, config) = prepared_restore()?;
        let mut operations = ScriptedOperations {
            fail_syncs: vec![1],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            restore_files_with_operations(&config, "complete", &mut operations),
            Err(Error::BackupIo(_))
        ));
        assert!(!restore_staging(&config).exists());
        restore_files(&config, "complete")?;
        Ok(())
    }

    #[test]
    fn restore_cleanup_failure_preserves_recovery_directories() -> TestResult {
        let (_root, config) = prepared_restore()?;
        let mut operations = ScriptedOperations {
            fail_renames: vec![3],
            fail_remove_dir_alls: vec![1],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            restore_files_with_operations(&config, "complete", &mut operations),
            Err(Error::Backup(RESTORE_RECOVERY_INCOMPLETE))
        ));
        assert!(restore_staging(&config).is_dir());
        assert!(!config.database_directory().as_path().join("alpha").exists());
        assert!(!config.database_directory().as_path().join("beta").exists());

        let (_root, config) = prepared_restore()?;
        let mut operations = ScriptedOperations {
            fail_syncs: vec![1],
            fail_remove_dir_alls: vec![1],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            restore_files_with_operations(&config, "complete", &mut operations),
            Err(Error::Backup(RESTORE_RECOVERY_INCOMPLETE))
        ));
        assert!(restore_staging(&config).is_dir());
        assert!(!config.database_directory().as_path().join("alpha").exists());

        let (_root, config) = prepared_restore()?;
        let mut operations = ScriptedOperations {
            fail_remove_dirs: vec![1],
            ..ScriptedOperations::default()
        };
        assert!(matches!(
            restore_files_with_operations(&config, "complete", &mut operations),
            Err(Error::Backup(RESTORE_RECOVERY_INCOMPLETE))
        ));
        assert!(restore_staging(&config).is_dir());
        assert!(
            config
                .database_directory()
                .as_path()
                .join("alpha")
                .is_file()
        );
        Ok(())
    }

    fn prepared_source(files: &[&str]) -> TestResult<(tempfile::TempDir, Config)> {
        let root = tempfile::tempdir()?;
        let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
        for file in files {
            fs::write(config.database_directory().as_path().join(file), file)?;
        }
        Ok((root, config))
    }

    fn prepared_restore() -> TestResult<(tempfile::TempDir, Config)> {
        let (root, source) = prepared_source(&["alpha", "beta", "gamma"])?;
        create(&source, "complete")?;
        let restore = Config::new(root.path().join("restore"), source.backup_root().as_path())?;
        Ok((root, restore))
    }

    fn backup_staging(config: &Config, name: &str) -> PathBuf {
        config.backup_root().as_path().join(format!(
            ".attention-turso-stage-{}-{name}",
            std::process::id()
        ))
    }

    fn restore_staging(config: &Config) -> PathBuf {
        config
            .database_directory()
            .as_path()
            .join(".attention-turso-restore-stage")
    }

    fn file_names(paths: &[PathBuf]) -> Vec<&str> {
        paths
            .iter()
            .map(|path| path.file_name().and_then(|name| name.to_str()).unwrap())
            .collect()
    }
}
