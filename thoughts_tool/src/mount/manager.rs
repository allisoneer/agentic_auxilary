use super::types::{MountInfo, MountOptions};
use crate::error::Result;
use crate::platform::{Platform, PlatformInfo};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use super::mergerfs::MergerfsManager;

#[cfg(target_os = "macos")]
use super::fuse_t::FuseTManager;

/// Trait for platform-specific mount operations
#[async_trait]
pub trait MountManager: Send + Sync {
    /// Mount multiple source directories to a target directory
    async fn mount(&self, sources: &[PathBuf], target: &Path, options: &MountOptions)
    -> Result<()>;

    /// Unmount a target directory
    async fn unmount(&self, target: &Path, force: bool) -> Result<()>;

    /// Check if a path is currently mounted
    async fn is_mounted(&self, target: &Path) -> Result<bool>;

    /// List all active mounts managed by this tool
    async fn list_mounts(&self) -> Result<Vec<MountInfo>>;

    /// Get detailed information about a specific mount
    async fn get_mount_info(&self, target: &Path) -> Result<Option<MountInfo>>;

    /// Remount with different sources (atomic operation)
    async fn remount(
        &self,
        sources: &[PathBuf],
        target: &Path,
        options: &MountOptions,
    ) -> Result<()> {
        // Default implementation: unmount then mount
        // Platform-specific implementations may override for atomic operations
        self.unmount(target, false).await?;
        self.mount(sources, target, options).await
    }

    /// Check if the mount system is available and properly configured
    async fn check_health(&self) -> Result<()>;

    /// Get platform-specific mount command for debugging
    fn get_mount_command(
        &self,
        sources: &[PathBuf],
        target: &Path,
        options: &MountOptions,
    ) -> String;
}

/// Factory function to get the appropriate mount manager for the current platform
pub fn get_mount_manager(platform_info: &PlatformInfo) -> Result<Box<dyn MountManager>> {
    match &platform_info.platform {
        #[cfg(target_os = "linux")]
        Platform::Linux(info) => {
            if !info.has_mergerfs {
                return Err(crate::error::ThoughtsError::ToolNotFound {
                    tool: "mergerfs".to_string(),
                });
            }
            if !info.fuse_available {
                return Err(crate::error::ThoughtsError::PlatformNotSupported {
                    platform: "Linux without FUSE support".to_string(),
                });
            }
            Ok(Box::new(MergerfsManager::new()))
        }
        #[cfg(target_os = "macos")]
        Platform::MacOS(info) => {
            if !info.has_fuse_t && !info.has_macfuse {
                return Err(crate::error::ThoughtsError::ToolNotFound {
                    tool: "FUSE-T or macFUSE".to_string(),
                });
            }
            Ok(Box::new(FuseTManager::new(info.clone())))
        }
        #[cfg(not(target_os = "linux"))]
        Platform::Linux(_) => Err(crate::error::ThoughtsError::PlatformNotSupported {
            platform: "Linux support not compiled in".to_string(),
        }),
        #[cfg(not(target_os = "macos"))]
        Platform::MacOS(_) => Err(crate::error::ThoughtsError::PlatformNotSupported {
            platform: "macOS support not compiled in".to_string(),
        }),
        Platform::Unsupported(os) => Err(crate::error::ThoughtsError::PlatformNotSupported {
            platform: os.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use super::get_mount_manager;
    #[cfg(target_os = "linux")]
    use crate::platform::LinuxInfo;
    #[cfg(target_os = "macos")]
    use crate::platform::MacOSInfo;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use crate::platform::{Platform, PlatformInfo};

    #[test]
    #[cfg(target_os = "linux")]
    fn test_get_mount_manager_linux() {
        let platform_info = PlatformInfo {
            platform: Platform::Linux(LinuxInfo {
                distro: "Ubuntu".to_string(),
                version: "22.04".to_string(),
                has_mergerfs: true,
                mergerfs_version: Some("2.33.5".to_string()),
                fuse_available: true,
                has_fusermount: false, // Testing without fusermount - should still work
            }),
            arch: "x86_64".to_string(),
            can_mount: true,
            missing_tools: vec![],
        };

        let manager = get_mount_manager(&platform_info);
        assert!(manager.is_ok());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_get_mount_manager_missing_tools() {
        let platform_info = PlatformInfo {
            platform: Platform::Linux(LinuxInfo {
                distro: "Ubuntu".to_string(),
                version: "22.04".to_string(),
                has_mergerfs: false,
                mergerfs_version: None,
                fuse_available: true,
                has_fusermount: true, // Even with fusermount, can't mount without mergerfs
            }),
            arch: "x86_64".to_string(),
            can_mount: false,
            missing_tools: vec!["mergerfs".to_string()],
        };

        let result = get_mount_manager(&platform_info);
        assert!(result.is_err());
        if let Err(e) = result {
            match e {
                crate::error::ThoughtsError::ToolNotFound { tool } => {
                    assert_eq!(tool, "mergerfs");
                }
                _ => panic!("Expected ToolNotFound error"),
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_get_mount_manager_macos() {
        let platform_info = PlatformInfo {
            platform: Platform::MacOS(MacOSInfo {
                version: "14.0".to_string(),
                has_fuse_t: true,
                fuse_t_version: Some("1.0.0".to_string()),
                has_macfuse: false,
                macfuse_version: None,
                has_unionfs: true,
                unionfs_path: Some(PathBuf::from("/usr/local/bin/unionfs-fuse")),
            }),
            arch: "aarch64".to_string(),
            can_mount: true,
            missing_tools: vec![],
        };

        let manager = get_mount_manager(&platform_info);
        assert!(manager.is_ok());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_get_mount_manager_no_fuse() {
        let platform_info = PlatformInfo {
            platform: Platform::MacOS(MacOSInfo {
                version: "14.0".to_string(),
                has_fuse_t: false,
                fuse_t_version: None,
                has_macfuse: false,
                macfuse_version: None,
                has_unionfs: false,
                unionfs_path: None,
            }),
            arch: "aarch64".to_string(),
            can_mount: false,
            missing_tools: vec!["FUSE-T".to_string()],
        };

        let result = get_mount_manager(&platform_info);
        assert!(result.is_err());
        if let Err(e) = result {
            match e {
                crate::error::ThoughtsError::ToolNotFound { tool } => {
                    assert_eq!(tool, "FUSE-T or macFUSE");
                }
                _ => panic!("Expected ToolNotFound error"),
            }
        }
    }
}
