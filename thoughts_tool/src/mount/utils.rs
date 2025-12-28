use crate::error::Result;
use crate::platform::common::MOUNT_POINT_PERMISSIONS;
use std::path::Path;
use tokio::fs;
use tracing::{debug, warn};

/// Ensure a mount point directory exists with proper permissions
pub async fn ensure_mount_point(path: &Path) -> Result<()> {
    if !path.exists() {
        debug!("Creating mount point directory: {}", path.display());
        fs::create_dir_all(path).await?;

        // Set permissions on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(MOUNT_POINT_PERMISSIONS);
            fs::set_permissions(path, permissions).await?;
        }
    } else if !path.is_dir() {
        return Err(crate::error::ThoughtsError::MountOperationFailed {
            message: format!("{} exists but is not a directory", path.display()),
        });
    }

    Ok(())
}

/// Clean up empty mount point directory after unmount
pub async fn cleanup_mount_point(path: &Path) -> Result<()> {
    if path.exists() && path.is_dir() {
        // Check if directory is empty
        let mut entries = fs::read_dir(path).await?;
        if entries.next_entry().await?.is_none() {
            debug!("Removing empty mount point: {}", path.display());
            match fs::remove_dir(path).await {
                Ok(()) => {}
                Err(e) => {
                    warn!("Failed to remove mount point {}: {}", path.display(), e);
                    // Not a critical error
                }
            }
        }
    }

    Ok(())
}

/// Check if a path is safe to use as a mount point
pub async fn validate_mount_point(path: &Path) -> Result<()> {
    let path_str = path.to_str().unwrap_or("");

    // First, check if path is under user's home directory or /tmp (allowed paths)
    if let Ok(home) = std::env::var("HOME")
        && path_str.starts_with(&home)
    {
        return Ok(());
    }

    // Also allow temp directories
    if path_str.starts_with("/tmp") || path_str.starts_with("/private/tmp") {
        return Ok(());
    }

    // Now check forbidden system directories
    let forbidden_paths = [
        "/",
        "/bin",
        "/boot",
        "/dev",
        "/etc",
        "/lib",
        "/lib64",
        "/opt",
        "/proc",
        "/root",
        "/sbin",
        "/sys",
        "/usr",
        "/var",
        "/System",
        "/Library",
        "/Applications",
        // Note: /Users is not forbidden as it contains home directories on macOS
        // Instead, we forbid specific system directories under /Users
        "/Users/Shared",
    ];

    for forbidden in &forbidden_paths {
        if path_str == *forbidden || path_str.starts_with(&format!("{forbidden}/")) {
            return Err(crate::error::ThoughtsError::MountOperationFailed {
                message: format!("Cannot mount on system directory: {}", path.display()),
            });
        }
    }

    // Allow any path that's not explicitly forbidden
    Ok(())
}

/// Normalize path for consistent mount operations
pub fn normalize_mount_path(path: &Path) -> Result<std::path::PathBuf> {
    use crate::utils::paths::expand_path;

    // Expand tilde
    let expanded = expand_path(path).map_err(crate::error::ThoughtsError::Other)?;

    // Canonicalize if possible (path must exist)
    if expanded.exists() {
        Ok(expanded.canonicalize()?)
    } else {
        // For non-existent paths, just normalize the components
        Ok(expanded)
    }
}

/// Poll an async check until it returns true or times out.
/// Returns:
/// - Ok(true) if the check succeeded within the timeout
/// - Ok(false) if the timeout elapsed without success
/// - Err(e) if the check returned an error
pub async fn verify_with_polling<F, Fut>(
    mut check: F,
    timeout: std::time::Duration,
    interval: std::time::Duration,
) -> Result<bool>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<bool>>,
{
    use std::time::Instant;
    use tokio::time::sleep;

    let deadline = Instant::now() + timeout;
    loop {
        match check().await {
            Ok(true) => return Ok(true),
            Ok(false) => {
                if Instant::now() >= deadline {
                    return Ok(false);
                }
                sleep(interval).await;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_ensure_mount_point() {
        let temp_dir = TempDir::new().unwrap();
        let mount_point = temp_dir.path().join("test_mount");

        // Should create directory
        assert!(!mount_point.exists());
        ensure_mount_point(&mount_point).await.unwrap();
        assert!(mount_point.exists());
        assert!(mount_point.is_dir());

        // Should be idempotent
        ensure_mount_point(&mount_point).await.unwrap();
        assert!(mount_point.exists());
    }

    #[tokio::test]
    async fn test_cleanup_mount_point() {
        let temp_dir = TempDir::new().unwrap();
        let mount_point = temp_dir.path().join("test_mount");

        // Create empty directory
        fs::create_dir(&mount_point).await.unwrap();

        // Should remove empty directory
        cleanup_mount_point(&mount_point).await.unwrap();
        assert!(!mount_point.exists());

        // Should handle non-existent directory
        cleanup_mount_point(&mount_point).await.unwrap();

        // Should not remove non-empty directory
        fs::create_dir(&mount_point).await.unwrap();
        fs::write(mount_point.join("file.txt"), "test")
            .await
            .unwrap();
        cleanup_mount_point(&mount_point).await.unwrap();
        assert!(mount_point.exists());
    }

    #[tokio::test]
    async fn test_validate_mount_point() {
        // System directories should be rejected
        assert!(
            validate_mount_point(Path::new("/etc/thoughts"))
                .await
                .is_err()
        );
        assert!(
            validate_mount_point(Path::new("/usr/local/thoughts"))
                .await
                .is_err()
        );

        // User directories should be allowed
        if let Ok(home) = std::env::var("HOME") {
            let user_path = Path::new(&home).join("thoughts");
            assert!(validate_mount_point(&user_path).await.is_ok());
        }

        // Temp directories should be allowed
        assert!(
            validate_mount_point(Path::new("/tmp/thoughts"))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn test_verify_with_polling_eventually_true() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use std::time::Duration;

        let counter = Arc::new(AtomicUsize::new(0));

        let counter_closure = {
            let counter = counter.clone();
            move || {
                let counter = counter.clone();
                async move {
                    let n = counter.fetch_add(1, Ordering::SeqCst);
                    Ok(n >= 3) // true on 4th call
                }
            }
        };

        let ok = super::verify_with_polling(
            counter_closure,
            Duration::from_millis(300),
            Duration::from_millis(5),
        )
        .await
        .unwrap();
        assert!(ok, "should become true before timeout");
    }

    #[tokio::test]
    async fn test_verify_with_polling_times_out() {
        use std::time::Duration;

        let ok = super::verify_with_polling(
            || async { Ok(false) },
            Duration::from_millis(50),
            Duration::from_millis(10),
        )
        .await
        .unwrap();
        assert!(!ok, "should time out and return Ok(false)");
    }

    #[tokio::test]
    async fn test_verify_with_polling_error_propagates() {
        use crate::error::ThoughtsError;
        use std::time::Duration;

        let err = super::verify_with_polling(
            || async {
                Err(ThoughtsError::MountOperationFailed {
                    message: "boom".into(),
                })
            },
            Duration::from_millis(50),
            Duration::from_millis(10),
        )
        .await
        .expect_err("expected error");
        assert!(format!("{err}").contains("boom"));
    }
}
