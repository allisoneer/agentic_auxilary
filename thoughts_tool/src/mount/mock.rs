use super::manager::MountManager;
use super::types::*;
use crate::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Mock mount manager for testing
#[derive(Clone)]
pub struct MockMountManager {
    mounts: Arc<Mutex<HashMap<PathBuf, MountInfo>>>,
    should_fail: Arc<Mutex<bool>>,
}

impl Default for MockMountManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MockMountManager {
    pub fn new() -> Self {
        Self {
            mounts: Arc::new(Mutex::new(HashMap::new())),
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_should_fail(&self, fail: bool) {
        *self.should_fail.lock().unwrap() = fail;
    }

    pub fn get_mounts(&self) -> HashMap<PathBuf, MountInfo> {
        self.mounts.lock().unwrap().clone()
    }
}

#[async_trait]
impl MountManager for MockMountManager {
    async fn mount(
        &self,
        sources: &[PathBuf],
        target: &Path,
        _options: &MountOptions,
    ) -> Result<()> {
        if *self.should_fail.lock().unwrap() {
            return Err(crate::error::ThoughtsError::MountOperationFailed {
                message: "Mock mount failed".to_string(),
            });
        }

        let mount_info = MountInfo {
            target: target.to_path_buf(),
            sources: sources.to_vec(),
            status: MountStatus::Mounted,
            fs_type: "mock".to_string(),
            options: vec!["mock".to_string()],
            mounted_at: Some(std::time::SystemTime::now()),
            pid: Some(std::process::id()),
            metadata: MountMetadata::Unknown,
        };

        self.mounts
            .lock()
            .unwrap()
            .insert(target.to_path_buf(), mount_info);
        Ok(())
    }

    async fn unmount(&self, target: &Path, _force: bool) -> Result<()> {
        if *self.should_fail.lock().unwrap() {
            return Err(crate::error::ThoughtsError::MountOperationFailed {
                message: "Mock unmount failed".to_string(),
            });
        }

        self.mounts.lock().unwrap().remove(target);
        Ok(())
    }

    async fn is_mounted(&self, target: &Path) -> Result<bool> {
        Ok(self.mounts.lock().unwrap().contains_key(target))
    }

    async fn list_mounts(&self) -> Result<Vec<MountInfo>> {
        Ok(self.mounts.lock().unwrap().values().cloned().collect())
    }

    async fn get_mount_info(&self, target: &Path) -> Result<Option<MountInfo>> {
        Ok(self.mounts.lock().unwrap().get(target).cloned())
    }

    async fn check_health(&self) -> Result<()> {
        if *self.should_fail.lock().unwrap() {
            return Err(crate::error::ThoughtsError::MountOperationFailed {
                message: "Mock health check failed".to_string(),
            });
        }
        Ok(())
    }

    fn get_mount_command(
        &self,
        _sources: &[PathBuf],
        _target: &Path,
        _options: &MountOptions,
    ) -> String {
        "mock mount command".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_mount_manager() {
        let manager = MockMountManager::new();
        let sources = vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")];
        let target = Path::new("/tmp/merged");
        let options = MountOptions::new();

        // Should not be mounted initially
        assert!(!manager.is_mounted(target).await.unwrap());

        // Mount should succeed
        manager.mount(&sources, target, &options).await.unwrap();
        assert!(manager.is_mounted(target).await.unwrap());

        // Should appear in list
        let mounts = manager.list_mounts().await.unwrap();
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].target, target);

        // Unmount should succeed
        manager.unmount(target, false).await.unwrap();
        assert!(!manager.is_mounted(target).await.unwrap());

        // Test failure mode
        manager.set_should_fail(true);
        assert!(manager.mount(&sources, target, &options).await.is_err());
    }
}
