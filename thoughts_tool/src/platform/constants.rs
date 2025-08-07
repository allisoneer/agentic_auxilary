/// Platform-specific constants for mount operations

#[cfg(target_os = "linux")]
pub mod linux {
    /// Default mount options for mergerfs
    pub const DEFAULT_MOUNT_OPTIONS: &[&str] = &[
        "category.create=mfs",
        "moveonenospc=true",
        "dropcacheonclose=true",
        "fsname=thoughts",
    ];

    /// Path to check for active mounts
    pub const PROC_MOUNTS: &str = "/proc/mounts";

    /// Path to mountinfo for detailed mount information
    pub const PROC_MOUNTINFO: &str = "/proc/self/mountinfo";

    /// Filesystem type identifier for mergerfs
    pub const MERGERFS_FSTYPE: &str = "fuse.mergerfs";
}

#[cfg(target_os = "macos")]
pub mod macos {
    /// Default volume name for FUSE mounts
    pub const DEFAULT_VOLUME_NAME: &str = "Thoughts";

    /// Default mount options for FUSE-T with unionfs-fuse
    pub const DEFAULT_MOUNT_OPTIONS: &[&str] = &[
        "volname=Thoughts",
        "local",
        "cow",
        "hide_meta_files",
        "use_ino",
        "max_files=32768",
    ];

    /// FUSE-T filesystem path
    pub const FUSE_T_FS_PATH: &str = "/Library/Filesystems/fuse-t.fs";

    /// unionfs-fuse binary names to search for
    pub const UNIONFS_BINARIES: &[&str] = &["unionfs-fuse", "unionfs"];

    /// Mount command for listing mounts
    pub const MOUNT_CMD: &str = "mount";

    /// Diskutil command for volume operations
    pub const DISKUTIL_CMD: &str = "diskutil";
}

/// Common constants across platforms
pub mod common {
    use std::time::Duration;

    /// Default permissions for mount point directories
    pub const MOUNT_POINT_PERMISSIONS: u32 = 0o755;

    /// Timeout for mount operations
    pub const MOUNT_TIMEOUT: Duration = Duration::from_secs(30);

    /// Timeout for unmount operations
    pub const UNMOUNT_TIMEOUT: Duration = Duration::from_secs(10);

    /// Maximum retries for mount operations
    pub const MAX_MOUNT_RETRIES: u32 = 3;

    /// Delay between mount retries
    pub const MOUNT_RETRY_DELAY: Duration = Duration::from_millis(500);
}
