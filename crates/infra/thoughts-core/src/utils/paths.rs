use anyhow::Result;
use anyhow::anyhow;
use dirs;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;

/// Expand tilde (~) in paths to home directory
pub fn expand_path(path: &Path) -> Result<PathBuf> {
    let path_str = path.to_string_lossy();

    if let Some(stripped) = path_str.strip_prefix("~/") {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        Ok(home.join(stripped))
    } else if path_str == "~" {
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))
    } else {
        Ok(path.to_path_buf())
    }
}

/// Ensure a directory exists, creating it if necessary
pub fn ensure_dir(path: &Path) -> Result<()> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(anyhow!(
            "Path exists but is not a directory: {}",
            path.display()
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => match std::fs::create_dir_all(path) {
            Ok(()) => Ok(()),
            Err(create_error) if create_error.kind() == ErrorKind::AlreadyExists => {
                Err(inaccessible_existing_path_error(path, &create_error))
            }
            Err(create_error) => Err(anyhow!(create_error)
                .context(format!("Failed to create directory: {}", path.display()))),
        },
        Err(error) if is_likely_stale_mount_error(&error) => Err(stale_mount_error(path, &error)),
        Err(error) => Err(anyhow!(error).context(format!(
            "Failed to access directory path: {}",
            path.display()
        ))),
    }
}

fn is_likely_stale_mount_error(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(107 | 116))
}

fn stale_mount_error(path: &Path, error: &std::io::Error) -> anyhow::Error {
    anyhow!(
        "Failed to access {}: mountpoint exists but is not accessible\n\
This may be a stale/disconnected FUSE mount (for example mergerfs; \"{}\").\n\
Repair or unmount/remount the stale mount, then run:\n\
  thoughts mount update\n\
or:\n\
  thoughts sync\n\
If this repository is already initialized, you likely do not need to run `thoughts init` again.",
        path.display(),
        error
    )
}

fn inaccessible_existing_path_error(path: &Path, error: &std::io::Error) -> anyhow::Error {
    anyhow!(
        "Failed to access {}: path already exists but is not accessible\n\
This may be a stale/disconnected FUSE mount (for example mergerfs; \"{}\").\n\
Repair or unmount/remount the stale mount, then run:\n\
  thoughts mount update\n\
or:\n\
  thoughts sync\n\
If this repository is already initialized, you likely do not need to run `thoughts init` again.",
        path.display(),
        error
    )
}

/// Sanitize a directory name for use in filesystem
pub fn sanitize_dir_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

// Add after line 50 (after sanitize_dir_name function)

/// Get the repository configuration file path
pub fn get_repo_config_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".thoughts").join("config.json")
}

/// Get external metadata directory for personal metadata about other repos
#[cfg(target_os = "macos")]
pub fn get_external_metadata_dir() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(home.join(".thoughts").join("data").join("external"))
}

/// Get local metadata file path for a repository
// TODO(2): Implement local metadata caching
pub fn get_local_metadata_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".thoughts").join("data").join("local.json")
}

/// Get rules file path for a repository
// TODO(2): Implement repository-specific rules system
pub fn get_repo_rules_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".thoughts").join("rules.json")
}

/// Get the XDG config home directory.
///
/// Returns `$XDG_CONFIG_HOME` if set, otherwise `~/.config`.
fn xdg_config_home() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(dir));
    }
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(home.join(".config"))
}

/// Get the repository mapping file path.
///
/// Returns the location at `~/.config/agentic/repos.json`.
pub fn get_repo_mapping_path() -> Result<PathBuf> {
    Ok(xdg_config_home()?.join("agentic").join("repos.json"))
}

/// Get the legacy repository mapping file path.
///
/// Returns the old location at `~/.thoughts/repos.json` for migration purposes.
pub fn get_legacy_repo_mapping_path() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(home.join(".thoughts").join("repos.json"))
}

/// Get the personal config path (for deprecation warnings)
pub fn get_personal_config_path() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(home.join(".thoughts").join("config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_expand_path() {
        // Test tilde expansion
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_path(Path::new("~/test")).unwrap(), home.join("test"));
        assert_eq!(expand_path(Path::new("~")).unwrap(), home);

        // Test absolute path
        assert_eq!(
            expand_path(Path::new("/tmp/test")).unwrap(),
            PathBuf::from("/tmp/test")
        );

        // Test relative path
        assert_eq!(
            expand_path(Path::new("test")).unwrap(),
            PathBuf::from("test")
        );
    }

    #[test]
    fn test_sanitize_dir_name() {
        assert_eq!(sanitize_dir_name("normal-name_123"), "normal-name_123");
        assert_eq!(
            sanitize_dir_name("bad/name:with*chars?"),
            "bad_name_with_chars_"
        );
    }

    #[test]
    fn test_ensure_dir_creates_missing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("new-dir");

        ensure_dir(&path).unwrap();

        assert!(path.is_dir());
    }

    #[test]
    fn test_ensure_dir_rejects_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("file");
        std::fs::write(&path, "not a directory").unwrap();

        let error = ensure_dir(&path).unwrap_err().to_string();

        assert!(error.contains("Path exists but is not a directory"));
        assert!(error.contains(&path.display().to_string()));
    }

    #[test]
    fn test_inaccessible_existing_path_error_is_actionable() {
        let path = Path::new(".thoughts-data/thoughts");
        let source_error = std::io::Error::from(ErrorKind::AlreadyExists);
        let error = inaccessible_existing_path_error(path, &source_error).to_string();

        assert!(error.contains(".thoughts-data/thoughts"));
        assert!(error.contains("path already exists but is not accessible"));
        assert!(error.contains("stale/disconnected FUSE mount"));
        assert!(error.contains("thoughts mount update"));
        assert!(error.contains("thoughts sync"));
        assert!(error.contains("do not need to run `thoughts init` again"));
    }
}
