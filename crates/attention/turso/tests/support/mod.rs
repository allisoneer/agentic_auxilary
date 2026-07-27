#![expect(dead_code)]

use std::error::Error;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;
use tempfile::TempDir;
use turso_db::Builder;
use turso_db::Database;

pub type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct FileDatabase {
    _root: TempDir,
    path: PathBuf,
    pub database: Database,
}

impl FileDatabase {
    pub async fn open() -> TestResult<Self> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("attention.db");
        let database = open_database(&path).await?;
        Ok(Self {
            _root: root,
            path,
            database,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub async fn open_database(path: &Path) -> TestResult<Database> {
    let path = path
        .to_str()
        .ok_or_else(|| "temporary database path is not UTF-8".to_string())?;
    Ok(Builder::new_local(path).build().await?)
}

pub fn regular_file_inventory(root: &Path) -> TestResult<Vec<(String, u64)>> {
    let mut inventory = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            inventory.push((
                entry.file_name().to_string_lossy().into_owned(),
                entry.metadata()?.len(),
            ));
        }
    }
    inventory.sort_unstable();
    Ok(inventory)
}

pub fn wait_for_file(path: &Path, timeout: Duration) -> TestResult {
    let started = Instant::now();
    while !path.exists() {
        if started.elapsed() >= timeout {
            return Err(format!("child did not reach barrier at {}", path.display()).into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

pub async fn pause_at(path: &Path) -> TestResult {
    fs::File::create(path)?;
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if fs::metadata(path).is_err() {
            return Ok(());
        }
    }
}
