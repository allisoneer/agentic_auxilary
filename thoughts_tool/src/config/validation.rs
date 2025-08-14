use crate::config::Mount;
use crate::error::{Result, ThoughtsError};
use crate::utils::paths;
use std::fs;
use std::path::Path;
use tracing::warn;

pub struct MountValidator;

impl MountValidator {
    /// Validate a mount configuration
    pub fn validate_mount(mount: &Mount) -> Result<()> {
        match mount {
            Mount::Directory { path, .. } => {
                // Validate directory mounts
                Self::validate_mount_directory(path)
            }
            Mount::Git { url, .. } => {
                // Validate git URL format
                Self::validate_git_url(url)
            }
        }
    }

    fn validate_mount_directory(path: &Path) -> Result<()> {
        let expanded_path = paths::expand_path(path)?;

        if !expanded_path.exists() {
            return Err(ThoughtsError::MountSourceNotFound {
                path: expanded_path,
            });
        }

        if !expanded_path.is_dir() {
            return Err(ThoughtsError::ConfigInvalid {
                message: format!("Mount path is not a directory: {}", expanded_path.display()),
            });
        }

        // Check path validity (no system directories, etc)
        Self::check_mount_path_validity(&expanded_path)?;

        // Check permissions
        validate_directory_permissions(&expanded_path)?;

        Ok(())
    }

    fn validate_git_url(url: &str) -> Result<()> {
        if url.is_empty() {
            return Err(ThoughtsError::ConfigInvalid {
                message: "Git URL cannot be empty".to_string(),
            });
        }

        // Basic URL validation
        let valid_prefixes = ["git@", "https://", "http://", "ssh://"];
        if !valid_prefixes.iter().any(|prefix| url.starts_with(prefix)) {
            return Err(ThoughtsError::ConfigInvalid {
                message: format!(
                    "Invalid git URL format: {url}. Must start with git@, https://, http://, or ssh://"
                ),
            });
        }

        Ok(())
    }

    // Keep existing check_mount_path_validity for directory validation
    pub fn check_mount_path_validity(path: &Path) -> Result<()> {
        let expanded = paths::expand_path(path)?;

        // Prevent mounting system directories
        let path_str = expanded.to_string_lossy();
        let forbidden_prefixes = [
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
            "/usr/bin",
            "/usr/sbin",
            "/var/log",
            "/var/lib",
            "/var/run",
            "/System",
            "/Library/System",
            "/private/etc",
            "/private/var",
        ];

        for prefix in &forbidden_prefixes {
            if path_str.starts_with(prefix) {
                return Err(ThoughtsError::MountPermissionDenied {
                    path: expanded.clone(),
                    reason: "Cannot use system directory as mount source".to_string(),
                });
            }
        }

        // Check if path is in user's home or temp directory
        if let Some(home) = dirs::home_dir() {
            if expanded.starts_with(&home) {
                return Ok(());
            }
        }

        // Allow /tmp and common temp directories
        if path_str.starts_with("/tmp")
            || path_str.starts_with("/var/tmp")
            || path_str.starts_with("/private/tmp")
        {
            return Ok(());
        }

        // Allow any other path with a warning
        warn!(
            "Mount source {} is outside home directory",
            expanded.display()
        );
        Ok(())
    }
}

/// Validate directory permissions
fn validate_directory_permissions(path: &Path) -> Result<()> {
    // Try to read directory contents
    match fs::read_dir(path) {
        Ok(_) => Ok(()),
        Err(e) => match e.kind() {
            std::io::ErrorKind::PermissionDenied => Err(ThoughtsError::MountPermissionDenied {
                path: path.to_path_buf(),
                reason: "Cannot read directory contents".to_string(),
            }),
            _ => Err(e.into()),
        },
    }
}

/// Sanitize a mount name for use as directory name
pub fn sanitize_mount_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SyncStrategy;
    use tempfile::TempDir;

    #[test]
    fn test_validate_mount() {
        let temp_dir = TempDir::new().unwrap();
        let mount_path = temp_dir.path().join("mount");
        fs::create_dir(&mount_path).unwrap();

        let mount = Mount::Directory {
            path: mount_path.clone(),
            sync: SyncStrategy::None,
        };

        assert!(MountValidator::validate_mount(&mount).is_ok());

        // Test non-existent path
        let bad_mount = Mount::Directory {
            path: PathBuf::from("/this/does/not/exist"),
            sync: SyncStrategy::None,
        };

        assert!(MountValidator::validate_mount(&bad_mount).is_err());

        // Test git mount validation
        let git_mount = Mount::Git {
            url: "git@github.com:user/repo.git".to_string(),
            sync: SyncStrategy::Auto,
            subpath: None,
        };

        assert!(MountValidator::validate_mount(&git_mount).is_ok());

        // Test invalid git URL
        let bad_git_mount = Mount::Git {
            url: "not-a-url".to_string(),
            sync: SyncStrategy::Auto,
            subpath: None,
        };

        assert!(MountValidator::validate_mount(&bad_git_mount).is_err());
    }

    #[test]
    fn test_check_mount_path_validity() {
        // System paths should be rejected
        assert!(MountValidator::check_mount_path_validity(Path::new("/etc/test")).is_err());
        assert!(MountValidator::check_mount_path_validity(Path::new("/usr/bin/test")).is_err());

        // Temp paths should be allowed
        assert!(MountValidator::check_mount_path_validity(Path::new("/tmp/test")).is_ok());

        // Home paths should be allowed
        if let Some(home) = dirs::home_dir() {
            let home_path = home.join("test");
            assert!(MountValidator::check_mount_path_validity(&home_path).is_ok());
        }
    }

    #[test]
    fn test_validate_git_url() {
        // Valid URLs
        assert!(MountValidator::validate_git_url("https://github.com/user/repo.git").is_ok());
        assert!(MountValidator::validate_git_url("git@github.com:user/repo.git").is_ok());
        assert!(MountValidator::validate_git_url("ssh://git@example.com/repo.git").is_ok());

        // Invalid URLs
        assert!(MountValidator::validate_git_url("").is_err());
        assert!(MountValidator::validate_git_url("not-a-url").is_err());
    }

    #[test]
    fn test_sanitize_mount_name() {
        assert_eq!(sanitize_mount_name("valid-name_123"), "valid-name_123");
        assert_eq!(sanitize_mount_name("bad name!@#"), "bad_name___");
        assert_eq!(sanitize_mount_name("CamelCase"), "CamelCase");
    }
}
