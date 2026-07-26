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
    let result = create_staged(config.database_directory().as_path(), &staging);
    match result {
        Ok(manifest) => {
            sync_directory(&staging)?;
            fs::rename(&staging, &final_path).map_err(Error::BackupIo)?;
            sync_directory(config.backup_root().as_path())?;
            Ok(manifest)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

pub fn restore_files(config: &Config, name: &str) -> Result<BackupManifest, Error> {
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
    let result = restore_staged(&backup, &staging, &manifest);
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    for entry in &manifest.files {
        fs::rename(
            staging.join(&entry.path),
            config.database_directory().as_path().join(&entry.path),
        )
        .map_err(Error::BackupIo)?;
    }
    fs::remove_dir(&staging).map_err(Error::BackupIo)?;
    sync_directory(config.database_directory().as_path())?;
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

fn restore_staged(backup: &Path, staging: &Path, manifest: &BackupManifest) -> Result<(), Error> {
    for entry in &manifest.files {
        let source = backup.join(&entry.path);
        let destination = staging.join(&entry.path);
        let (size, checksum) = copy_and_hash(&source, &destination)?;
        if size != entry.size || checksum != entry.checksum {
            return Err(Error::Backup("backup changed while restoring"));
        }
    }
    sync_directory(staging)
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

fn sync_directory(path: &Path) -> Result<(), Error> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(Error::BackupIo)?;
    Ok(())
}
