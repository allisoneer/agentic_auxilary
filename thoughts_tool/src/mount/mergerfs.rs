use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

use super::manager::MountManager;
use super::types::*;
use super::utils;
use crate::error::{Result, ThoughtsError};
use crate::platform::common::*;
use crate::platform::linux::*;

pub struct MergerfsManager {
    /// Path to mergerfs binary (cached)
    mergerfs_path: PathBuf,
    /// Path to fusermount binary (cached, if available)
    fusermount_path: Option<PathBuf>,
}

impl MergerfsManager {
    pub fn new() -> Self {
        // Platform detection already verified mergerfs exists
        // No need to duplicate the check here
        let mergerfs_path = PathBuf::from("mergerfs");

        // Try to find fusermount or fusermount3 for unmounting
        // This is optional - we can fall back to umount if not available
        let fusermount_path = which::which("fusermount")
            .or_else(|_| which::which("fusermount3"))
            .ok();

        Self {
            mergerfs_path,
            fusermount_path,
        }
    }

    /// Build mergerfs command arguments
    fn build_mount_args(
        &self,
        sources: &[PathBuf],
        target: &Path,
        options: &MountOptions,
    ) -> Vec<String> {
        let mut args = Vec::new();

        // Add -o flag
        args.push("-o".to_string());

        // Build options string
        let mut opts = Vec::new();

        // Add default options
        opts.extend(DEFAULT_MOUNT_OPTIONS.iter().map(|s| s.to_string()));

        // Add read-only if requested
        if options.read_only {
            opts.push("ro".to_string());
        }

        // Add allow_other if requested
        if options.allow_other {
            opts.push("allow_other".to_string());
        }

        // Add any extra options
        opts.extend(options.extra_options.clone());

        // Join all options
        args.push(opts.join(","));

        // Add source directories (colon-separated)
        let source_str = sources
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(":");
        args.push(source_str);

        // Add target directory
        args.push(target.display().to_string());

        args
    }

    /// Parse /proc/mounts to find mergerfs mounts
    async fn parse_proc_mounts(&self) -> Result<Vec<MountInfo>> {
        use tokio::fs;

        let content = fs::read_to_string(PROC_MOUNTS).await?;
        let mut mounts = Vec::new();

        for line in content.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 6 {
                continue;
            }

            let fs_type = fields[2];
            if fs_type != MERGERFS_FSTYPE {
                continue;
            }

            let sources_str = fields[0];
            let target = PathBuf::from(fields[1]);
            let options = fields[3].split(',').map(|s| s.to_string()).collect();

            // Parse source directories (colon-separated)
            let sources: Vec<PathBuf> = sources_str.split(':').map(PathBuf::from).collect();

            mounts.push(MountInfo {
                target,
                sources,
                status: MountStatus::Mounted,
                fs_type: fs_type.to_string(),
                options,
                mounted_at: None, // Would need to parse /proc/self/mountinfo for this
                pid: None,
                metadata: MountMetadata::Linux {
                    mount_id: None,
                    parent_id: None,
                    major_minor: None,
                },
            });
        }

        Ok(mounts)
    }

    /// Get detailed mount information from /proc/self/mountinfo
    async fn get_detailed_mount_info(&self, target: &Path) -> Result<Option<MountInfo>> {
        use tokio::fs;

        let target_canon = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());

        let content = match fs::read_to_string(PROC_MOUNTINFO).await {
            Ok(c) => c,
            Err(_) => {
                // Fall back to basic info from /proc/mounts
                let mounts = self.parse_proc_mounts().await?;
                return Ok(mounts.into_iter().find(|m| {
                    let mt = std::fs::canonicalize(&m.target).unwrap_or_else(|_| m.target.clone());
                    mt == target_canon
                }));
            }
        };

        for line in content.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 {
                continue;
            }

            // Format: mount_id parent_id major:minor root mount_point options...
            let mount_id: u32 = fields[0].parse().unwrap_or(0);
            let parent_id: u32 = fields[1].parse().unwrap_or(0);
            let major_minor = fields[2].to_string();

            // Find the separator
            let separator_pos = fields
                .iter()
                .position(|&f| f == "-")
                .unwrap_or(fields.len());
            if separator_pos + 3 >= fields.len() {
                continue;
            }

            let mount_point = PathBuf::from(fields[4]);
            let mount_point_canon =
                std::fs::canonicalize(&mount_point).unwrap_or_else(|_| mount_point.clone());
            if mount_point_canon != target_canon {
                continue;
            }

            let fs_type = fields[separator_pos + 1];
            if fs_type != MERGERFS_FSTYPE {
                continue;
            }

            let sources_str = fields[separator_pos + 2];
            let sources: Vec<PathBuf> = sources_str.split(':').map(PathBuf::from).collect();

            // Parse options
            let options: Vec<String> = fields[5..separator_pos]
                .iter()
                .flat_map(|o| o.split(','))
                .map(|s| s.to_string())
                .collect();

            return Ok(Some(MountInfo {
                target: mount_point,
                sources,
                status: MountStatus::Mounted,
                fs_type: fs_type.to_string(),
                options,
                mounted_at: None,
                pid: None,
                metadata: MountMetadata::Linux {
                    mount_id: Some(mount_id),
                    parent_id: Some(parent_id),
                    major_minor: Some(major_minor),
                },
            }));
        }

        Ok(None)
    }

    /// Helper method to unmount using umount command
    async fn unmount_with_umount(&self, target: &Path, force: bool) -> Result<()> {
        let mut cmd = tokio::process::Command::new("umount");

        if force {
            cmd.arg("-l"); // Lazy unmount
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
            return Err(ThoughtsError::MountOperationFailed {
                message: format!("umount failed: {stderr}"),
            });
        }

        Ok(())
    }
}

#[async_trait]
impl MountManager for MergerfsManager {
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

        let args = self.build_mount_args(sources, target, options);
        let _timeout = options.timeout.unwrap_or(MOUNT_TIMEOUT);

        info!("Mounting {} sources to {}", sources.len(), target.display());
        debug!(
            "Mount command: {} {}",
            self.mergerfs_path.display(),
            args.join(" ")
        );

        // Try mounting with retries
        for attempt in 0..=options.retries {
            if attempt > 0 {
                warn!("Mount attempt {} of {}", attempt + 1, options.retries + 1);
                sleep(MOUNT_RETRY_DELAY).await;
            }

            let start = Instant::now();
            let output = tokio::process::Command::new(&self.mergerfs_path)
                .args(&args)
                .output()
                .await?;

            let duration = start.elapsed();

            if output.status.success() {
                info!("Successfully mounted in {:?}", duration);

                // Verify mount succeeded
                sleep(Duration::from_millis(100)).await;
                if self.is_mounted(target).await? {
                    return Ok(());
                } else {
                    warn!("Mount command succeeded but mount not found");
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!("Mount failed: {}", stderr);

                if attempt == options.retries {
                    return Err(ThoughtsError::MountOperationFailed {
                        message: format!("mergerfs mount failed: {stderr}"),
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

        // Try fusermount first if available
        if let Some(fusermount_path) = &self.fusermount_path {
            let mut cmd = tokio::process::Command::new(fusermount_path);
            cmd.arg("-u");

            if force {
                cmd.arg("-z"); // Lazy unmount
            }

            cmd.arg(target);

            let output = timeout(UNMOUNT_TIMEOUT, cmd.output())
                .await
                .map_err(|_| ThoughtsError::CommandTimeout {
                    command: "fusermount".to_string(),
                    timeout_secs: UNMOUNT_TIMEOUT.as_secs(),
                })?
                .map_err(ThoughtsError::from)?;

            if output.status.success() {
                // Success with fusermount, continue to cleanup
                info!(
                    "Successfully unmounted {} with fusermount",
                    target.display()
                );
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("fusermount failed: {}, trying umount", stderr);

                // Fall through to try umount
                self.unmount_with_umount(target, force).await?;
            }
        } else {
            // No fusermount available, use umount directly
            debug!("fusermount not available, using umount");
            self.unmount_with_umount(target, force).await?;
        }

        // Clean up mount point if empty
        utils::cleanup_mount_point(target).await?;

        info!("Successfully unmounted {}", target.display());
        Ok(())
    }

    async fn is_mounted(&self, target: &Path) -> Result<bool> {
        let mounts = self.parse_proc_mounts().await?;
        let target_canon = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
        Ok(mounts.iter().any(|m| {
            let mt = std::fs::canonicalize(&m.target).unwrap_or_else(|_| m.target.clone());
            mt == target_canon
        }))
    }

    async fn list_mounts(&self) -> Result<Vec<MountInfo>> {
        self.parse_proc_mounts().await
    }

    async fn get_mount_info(&self, target: &Path) -> Result<Option<MountInfo>> {
        self.get_detailed_mount_info(target).await
    }

    async fn check_health(&self) -> Result<()> {
        // Check if mergerfs binary exists and is executable
        if !self.mergerfs_path.exists() {
            return Err(ThoughtsError::ToolNotFound {
                tool: "mergerfs".to_string(),
            });
        }

        // Check if FUSE is available
        if !Path::new("/dev/fuse").exists() {
            return Err(ThoughtsError::MountOperationFailed {
                message: "FUSE device not found. Is FUSE kernel module loaded?".to_string(),
            });
        }

        // Try to get version
        let output = Command::new(&self.mergerfs_path).arg("-V").output()?;

        if !output.status.success() {
            return Err(ThoughtsError::MountOperationFailed {
                message: "Failed to get mergerfs version".to_string(),
            });
        }

        debug!("mergerfs health check passed");
        Ok(())
    }

    fn get_mount_command(
        &self,
        sources: &[PathBuf],
        target: &Path,
        options: &MountOptions,
    ) -> String {
        let args = self.build_mount_args(sources, target, options);
        format!("{} {}", self.mergerfs_path.display(), args.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_mount_args() {
        let manager = MergerfsManager::new();
        let sources = vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")];
        let target = Path::new("/mnt/merged");
        let options = MountOptions {
            read_only: true,
            ..Default::default()
        };

        let args = manager.build_mount_args(&sources, target, &options);

        assert_eq!(args[0], "-o");
        assert!(args[1].contains("category.create=mfs"));
        assert!(args[1].contains("ro"));
        assert_eq!(args[2], "/tmp/a:/tmp/b");
        assert_eq!(args[3], "/mnt/merged");
    }

    #[tokio::test]
    async fn test_mount_validation() {
        let manager = MergerfsManager::new();
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
