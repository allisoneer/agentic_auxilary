use anyhow::Result;
use dirs;
use std::path::{Path, PathBuf};

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
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
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

/// Get the personal configuration file path
pub fn get_personal_config_path() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(home.join(".thoughts").join("config.json"))
}

/// Get external metadata directory for personal metadata about other repos
#[cfg(target_os = "macos")]
pub fn get_external_metadata_dir() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(home.join(".thoughts").join("data").join("external"))
}

/// Get local metadata file path for a repository
#[allow(dead_code)]
// TODO(2): Implement local metadata caching
pub fn get_local_metadata_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".thoughts").join("data").join("local.json")
}

/// Get rules file path for a repository
#[allow(dead_code)]
// TODO(2): Implement repository-specific rules system
pub fn get_repo_rules_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".thoughts").join("rules.json")
}

/// Get the repository mapping file path
pub fn get_repo_mapping_path() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(home.join(".thoughts").join("repos.json"))
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
}
