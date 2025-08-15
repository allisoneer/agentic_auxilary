use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

use super::manager::MountManager;
use super::types::*;
use super::utils;
use crate::error::{Result, ThoughtsError};
use crate::platform::common::{MOUNT_RETRY_DELAY, MOUNT_TIMEOUT, UNMOUNT_TIMEOUT};
use crate::platform::detector::MacOSInfo;
use crate::platform::macos::{DEFAULT_VOLUME_NAME, DISKUTIL_CMD, MOUNT_CMD};

pub struct FuseTManager {
    /// Platform information
    platform_info: MacOSInfo,

    /// Path to unionfs-fuse binary
    unionfs_path: Option<PathBuf>,
}

impl FuseTManager {
    pub fn new(platform_info: MacOSInfo) -> Self {
        // Platform detection already verified FUSE-T or macFUSE exists
        // Still need to check for unionfs-fuse as it's a separate tool
        let unionfs_path = which::which("unionfs-fuse")
            .or_else(|_| which::which("unionfs"))
            .ok();

        Self {
            platform_info,
            unionfs_path,
        }
    }

    /// Get human-readable FUSE implementation name
    fn get_fuse_implementation(&self) -> &'static str {
        if self.platform_info.has_fuse_t {
            "FUSE-T"
        } else if self.platform_info.has_macfuse {
            "macFUSE"
        } else {
            "No FUSE implementation"
        }
    }

    /// Build unionfs-fuse command for FUSE-T
    /// FUSE-T provides the FUSE layer, unionfs-fuse provides the union filesystem
    fn build_mount_command(
        &self,
        sources: &[PathBuf],
        target: &Path,
        options: &MountOptions,
    ) -> Result<(String, Vec<String>)> {
        // Ensure unionfs-fuse is available
        let unionfs_path =
            self.unionfs_path
                .as_ref()
                .ok_or_else(|| {
                    ThoughtsError::ToolNotFound {
                tool:
                    "unionfs-fuse (install from: https://github.com/WaterJuice/unionfs-fuse-macos)"
                        .to_string(),
            }
                })?;

        let mut args = Vec::new();

        // Build source directories string
        // Format: /dir1=RW:/dir2=RO
        let source_str = sources
            .iter()
            .enumerate()
            .map(|(i, p)| {
                // First directory is read-write (unless read_only is set)
                let mode = if i == 0 && !options.read_only {
                    "RW"
                } else {
                    "RO"
                };
                format!("{}={}", p.display(), mode)
            })
            .collect::<Vec<_>>()
            .join(":");

        // Build options
        args.push("-o".to_string());

        let mut opts = Vec::new();

        // Volume name for FUSE-T
        let default_volname = DEFAULT_VOLUME_NAME.to_string();
        let volname = options.volume_name.as_ref().unwrap_or(&default_volname);
        opts.push(format!("volname={}", volname));

        // Enable copy-on-write for better performance
        opts.push("cow".to_string());

        // Hide unionfs metadata files
        opts.push("hide_meta_files".to_string());

        // Increase max files for large directory trees
        opts.push("max_files=32768".to_string());

        // Add default FUSE-T options
        if options.allow_other {
            opts.push("allow_other".to_string());
        }

        // Use ino for better inode handling
        opts.push("use_ino".to_string());

        // Add extra options
        opts.extend(options.extra_options.clone());

        // Join all options
        args.push(opts.join(","));

        // Source directories as positional argument
        args.push(source_str);

        // Target mount point
        args.push(target.display().to_string());

        Ok((unionfs_path.display().to_string(), args))
    }

    /// Parse mount command output to find active mounts
    async fn parse_mount_output(&self) -> Result<Vec<MountInfo>> {
        let output = tokio::process::Command::new(MOUNT_CMD).output().await?;

        if !output.status.success() {
            return Err(ThoughtsError::MountOperationFailed {
                message: "Failed to list mounts".to_string(),
            });
        }

        // Load mount cache to get source information
        #[cfg(target_os = "macos")]
        let mount_cache = self.load_mount_cache().await.ok().flatten();

        let mut mounts = Vec::new();
        let output_str = String::from_utf8_lossy(&output.stdout);

        for line in output_str.lines() {
            // Format: unionfs-fuse on /path/to/mount (fuse-t, local, synchronous)
            // or: unionfs on /path/to/mount (macfuse, local, synchronous)

            // Parse the mount line
            if let Some(on_pos) = line.find(" on ") {
                let device = &line[..on_pos];

                // Only process unionfs mounts
                if !device.contains("unionfs") {
                    continue;
                }
                let rest = &line[on_pos + 4..];

                if let Some(paren_pos) = rest.find(" (") {
                    let mount_point = &rest[..paren_pos];
                    let options_str = &rest[paren_pos + 2..rest.len() - 1];
                    let options: Vec<String> =
                        options_str.split(", ").map(|s| s.to_string()).collect();

                    // Get sources from cache if available, otherwise use placeholder
                    let sources = {
                        #[cfg(target_os = "macos")]
                        if let Some(ref cache) = mount_cache {
                            if let Some(cached_info) = cache.mounts.get(&PathBuf::from(mount_point))
                            {
                                cached_info.sources.clone()
                            } else {
                                vec![PathBuf::from("<merged>")]
                            }
                        } else {
                            vec![PathBuf::from("<merged>")]
                        }

                        #[cfg(not(target_os = "macos"))]
                        vec![PathBuf::from("<merged>")]
                    };

                    let fs_type = options
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "fusefs".to_string());

                    mounts.push(MountInfo {
                        target: PathBuf::from(mount_point),
                        sources,
                        status: MountStatus::Mounted,
                        fs_type,
                        options,
                        mounted_at: None,
                        pid: None,
                        metadata: MountMetadata::MacOS {
                            volume_name: None,
                            volume_uuid: None,
                            disk_identifier: Some(device.to_string()),
                        },
                    });
                }
            }
        }

        Ok(mounts)
    }

    /// Get volume information using diskutil
    async fn get_volume_info(&self, target: &Path) -> Result<Option<(String, String)>> {
        let output = tokio::process::Command::new(DISKUTIL_CMD)
            .args(&["info", target.to_str().unwrap_or("")])
            .output()
            .await?;

        if !output.status.success() {
            return Ok(None);
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut volume_name = None;
        let mut volume_uuid = None;

        for line in output_str.lines() {
            if let Some(name) = line.strip_prefix("   Volume Name:").map(|s| s.trim()) {
                if !name.is_empty() && name != "Not applicable (no file system)" {
                    volume_name = Some(name.to_string());
                }
            } else if let Some(uuid) = line.strip_prefix("   Volume UUID:").map(|s| s.trim()) {
                if !uuid.is_empty() && uuid != "Not applicable (no file system)" {
                    volume_uuid = Some(uuid.to_string());
                }
            }
        }

        match (volume_name, volume_uuid) {
            (Some(name), Some(uuid)) => Ok(Some((name, uuid))),
            _ => Ok(None),
        }
    }

    /// Store mount state for persistence
    #[cfg(target_os = "macos")]
    async fn store_mount_state(
        &self,
        sources: &[PathBuf],
        target: &Path,
        options: &MountOptions,
        cmd_path: &str,
        args: &[String],
    ) -> Result<()> {
        use super::types::{CachedMountInfo, MountStateCache};
        use std::time::SystemTime;

        let cache_path = crate::utils::paths::get_external_metadata_dir()?.join("mount_state.json");

        // Load existing cache or create new one
        let mut cache = if cache_path.exists() {
            let content = tokio::fs::read_to_string(&cache_path).await?;
            serde_json::from_str(&content).unwrap_or_else(|_| MountStateCache {
                version: "1.0".to_string(),
                mounts: std::collections::HashMap::new(),
            })
        } else {
            MountStateCache {
                version: "1.0".to_string(),
                mounts: std::collections::HashMap::new(),
            }
        };

        // Add current mount info
        let mount_info = CachedMountInfo {
            target: target.to_path_buf(),
            sources: sources.to_vec(),
            mount_options: options.clone(),
            created_at: SystemTime::now(),
            mount_command: format!("{} {}", cmd_path, args.join(" ")),
            pid: None, // Could get this from process if needed
        };

        cache.mounts.insert(target.to_path_buf(), mount_info);

        // Ensure directory exists
        if let Some(parent) = cache_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Save cache
        let content = serde_json::to_string_pretty(&cache)?;
        tokio::fs::write(&cache_path, content).await?;

        Ok(())
    }

    /// Load cached mount sources
    #[cfg(target_os = "macos")]
    async fn load_mount_cache(&self) -> Result<Option<super::types::MountStateCache>> {
        let cache_path = crate::utils::paths::get_external_metadata_dir()?.join("mount_state.json");

        if !cache_path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&cache_path).await?;
        let cache = serde_json::from_str(&content)?;
        Ok(Some(cache))
    }
}

#[async_trait]
impl MountManager for FuseTManager {
    async fn mount(
        &self,
        sources: &[PathBuf],
        target: &Path,
        options: &MountOptions,
    ) -> Result<()> {
        // Validate inputs
        if sources.is_empty() {
            return Err(ThoughtsError::MountOperationFailed {
                message: "No source directories provided".to_string(),
            });
        }

        // Ensure all source directories exist
        for source in sources {
            if !source.exists() {
                return Err(ThoughtsError::MountSourceNotFound {
                    path: source.clone(),
                });
            }
        }

        // Validate mount point first
        utils::validate_mount_point(target).await?;

        // Ensure target directory exists
        utils::ensure_mount_point(target).await?;

        // Check if already mounted
        if self.is_mounted(target).await? {
            info!("Target is already mounted: {}", target.display());
            return Ok(());
        }

        let (cmd_path, args) = self.build_mount_command(sources, target, options)?;
        let _timeout = options.timeout.unwrap_or(MOUNT_TIMEOUT);

        info!(
            "Mounting {} sources to {} using {} + unionfs-fuse",
            sources.len(),
            target.display(),
            self.get_fuse_implementation()
        );
        debug!("Mount command: {} {}", cmd_path, args.join(" "));

        // Try mounting with retries
        for attempt in 0..=options.retries {
            if attempt > 0 {
                warn!("Mount attempt {} of {}", attempt + 1, options.retries + 1);
                sleep(MOUNT_RETRY_DELAY).await;
            }

            let start = Instant::now();
            let output = tokio::process::Command::new(&cmd_path)
                .args(&args)
                .output()
                .await?;

            let duration = start.elapsed();

            if output.status.success() {
                info!("Successfully mounted in {:?}", duration);

                // Verify mount succeeded
                sleep(Duration::from_millis(500)).await; // macOS needs more time
                if self.is_mounted(target).await? {
                    // Store mount state for macOS
                    #[cfg(target_os = "macos")]
                    {
                        if let Err(e) = self
                            .store_mount_state(sources, target, options, &cmd_path, &args)
                            .await
                        {
                            warn!("Failed to store mount state: {}", e);
                        }
                    }
                    return Ok(());
                } else {
                    warn!("Mount command succeeded but mount not found");
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                error!("Mount failed: stderr={}, stdout={}", stderr, stdout);

                if attempt == options.retries {
                    return Err(ThoughtsError::MountOperationFailed {
                        message: format!("unionfs mount failed: {}", stderr),
                    });
                }
            }
        }

        Err(ThoughtsError::MountOperationFailed {
            message: "Mount failed after all retries".to_string(),
        })
    }

    async fn unmount(&self, target: &Path, force: bool) -> Result<()> {
        if !self.is_mounted(target).await? {
            debug!("Target is not mounted: {}", target.display());
            return Ok(());
        }

        info!("Unmounting {}", target.display());

        let mut cmd = tokio::process::Command::new("umount");

        if force {
            cmd.arg("-f"); // Force unmount
        }

        cmd.arg(target);

        let output = timeout(UNMOUNT_TIMEOUT, cmd.output())
            .await
            .map_err(|_| ThoughtsError::CommandTimeout {
                command: "umount".to_string(),
                timeout_secs: UNMOUNT_TIMEOUT.as_secs(),
            })?
            .map_err(ThoughtsError::from)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Try diskutil eject as fallback
            if force {
                warn!("umount failed, trying diskutil eject: {}", stderr);
                let eject_output = timeout(
                    UNMOUNT_TIMEOUT,
                    tokio::process::Command::new(DISKUTIL_CMD)
                        .args(&["unmount", "force", target.to_str().unwrap_or("")])
                        .output(),
                )
                .await
                .map_err(|_| ThoughtsError::CommandTimeout {
                    command: "diskutil unmount".to_string(),
                    timeout_secs: UNMOUNT_TIMEOUT.as_secs(),
                })?
                .map_err(ThoughtsError::from)?;

                if !eject_output.status.success() {
                    return Err(ThoughtsError::MountOperationFailed {
                        message: format!("unmount failed: {}", stderr),
                    });
                }
            } else {
                return Err(ThoughtsError::MountOperationFailed {
                    message: format!("umount failed: {}", stderr),
                });
            }
        }

        // Clean up mount point if empty
        utils::cleanup_mount_point(target).await?;

        info!("Successfully unmounted {}", target.display());
        Ok(())
    }

    async fn is_mounted(&self, target: &Path) -> Result<bool> {
        let mounts = self.parse_mount_output().await?;
        Ok(mounts.iter().any(|m| m.target == target))
    }

    async fn list_mounts(&self) -> Result<Vec<MountInfo>> {
        self.parse_mount_output().await
    }

    async fn get_mount_info(&self, target: &Path) -> Result<Option<MountInfo>> {
        let mounts = self.parse_mount_output().await?;

        if let Some(mut mount) = mounts.into_iter().find(|m| m.target == target) {
            // Try to get additional volume information
            if let Ok(Some((name, uuid))) = self.get_volume_info(target).await {
                if let MountMetadata::MacOS {
                    ref mut volume_name,
                    ref mut volume_uuid,
                    ..
                } = mount.metadata
                {
                    *volume_name = Some(name);
                    *volume_uuid = Some(uuid);
                }
            }

            Ok(Some(mount))
        } else {
            Ok(None)
        }
    }

    async fn check_health(&self) -> Result<()> {
        // Check if we have FUSE support (FUSE-T preferred)
        if !self.platform_info.has_fuse_t && !self.platform_info.has_macfuse {
            return Err(ThoughtsError::ToolNotFound {
                tool: "FUSE-T (install from: https://www.fuse-t.org) or macFUSE".to_string(),
            });
        }

        // Check if unionfs-fuse is available
        if self.unionfs_path.is_none() {
            return Err(ThoughtsError::ToolNotFound {
                tool:
                    "unionfs-fuse (install from: https://github.com/WaterJuice/unionfs-fuse-macos)"
                        .to_string(),
            });
        }

        // Verify the binary is executable
        if let Some(path) = &self.unionfs_path {
            use std::os::unix::fs::PermissionsExt;
            let metadata = tokio::fs::metadata(path).await?;
            let permissions = metadata.permissions();
            if permissions.mode() & 0o111 == 0 {
                return Err(ThoughtsError::MountOperationFailed {
                    message: format!("unionfs-fuse at {} is not executable", path.display()),
                });
            }
        }

        info!(
            "FUSE health check passed: {} + unionfs-fuse",
            self.get_fuse_implementation()
        );
        Ok(())
    }

    fn get_mount_command(
        &self,
        sources: &[PathBuf],
        target: &Path,
        options: &MountOptions,
    ) -> String {
        match self.build_mount_command(sources, target, options) {
            Ok((cmd, args)) => format!("{} {}", cmd, args.join(" ")),
            Err(_) => "<unionfs-fuse not available>".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::detector::MacOSInfo;

    fn test_platform_info() -> MacOSInfo {
        MacOSInfo {
            version: "14.0".to_string(),
            has_fuse_t: true,
            fuse_t_version: Some("1.0.0".to_string()),
            has_macfuse: false,
            macfuse_version: None,
            has_unionfs: true,
            unionfs_path: Some(PathBuf::from("/usr/local/bin/unionfs-fuse")),
        }
    }

    #[test]
    fn test_get_fuse_implementation() {
        let manager = FuseTManager::new(test_platform_info());
        assert_eq!(manager.get_fuse_implementation(), "FUSE-T");

        let mut info = test_platform_info();
        info.has_fuse_t = false;
        info.has_macfuse = true;
        let manager = FuseTManager::new(info);
        assert_eq!(manager.get_fuse_implementation(), "macFUSE");
    }

    #[tokio::test]
    async fn test_mount_validation() {
        let manager = FuseTManager::new(test_platform_info());
        let target = Path::new("/tmp/test_mount");
        let options = MountOptions::default();

        // Test with empty sources
        let result = manager.mount(&[], target, &options).await;
        assert!(result.is_err());

        // Test with non-existent source
        let sources = vec![PathBuf::from("/this/does/not/exist")];
        let result = manager.mount(&sources, target, &options).await;
        assert!(result.is_err());
    }
}
