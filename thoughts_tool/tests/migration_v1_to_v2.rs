#![cfg(test)]

use anyhow::Result;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use thoughts_tool::{MountSpace, RepoConfigManager};

/// Create a v1 config file in the test repository
fn create_v1_config(repo_dir: &PathBuf) -> Result<()> {
    let config_dir = repo_dir.join(".thoughts");
    fs::create_dir_all(&config_dir)?;

    let v1_config = json!({
        "version": "1.0",
        "mount_dirs": {
            "repository": "context",
            "personal": "personal"
        },
        "requires": [
            {
                "remote": "https://github.com/team/docs.git",
                "mount_path": "team-docs",
                "description": "Team documentation",
                "sync": "auto"
            },
            {
                "remote": "git@github.com:company/api-docs.git",
                "mount_path": "api",
                "description": "API documentation",
                "subpath": "docs/api",
                "sync": "auto",
                "optional": true
            },
            {
                "remote": "https://github.com/rust-lang/rust.git",
                "mount_path": "references/rust",
                "description": "Rust language reference",
                "sync": "none"
            },
            {
                "remote": "https://github.com/tokio-rs/tokio.git",
                "mount_path": "tokio-reference",
                "description": "Tokio async runtime reference",
                "sync": "none"
            }
        ]
    });

    let config_path = config_dir.join("config.json");
    fs::write(&config_path, serde_json::to_string_pretty(&v1_config)?)?;
    Ok(())
}

#[test]
fn test_v1_config_loads_as_desired_state() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_dir = temp_dir.path().to_path_buf();

    // Create v1 config
    create_v1_config(&repo_dir)?;

    // Load via RepoConfigManager
    let manager = RepoConfigManager::new(repo_dir);
    let desired_state = manager.load_desired_state()?.expect("Should load config");

    // Verify it was detected as v1
    assert!(desired_state.was_v1, "Should detect as v1 config");

    // Verify mount directories are set to defaults
    assert_eq!(desired_state.mount_dirs.thoughts, "thoughts");
    assert_eq!(desired_state.mount_dirs.context, "context");
    assert_eq!(desired_state.mount_dirs.references, "references");

    // Verify no thoughts mount (requires explicit v2 config)
    assert!(desired_state.thoughts_mount.is_none());

    // Verify context mounts
    assert_eq!(desired_state.context_mounts.len(), 2);
    let team_docs = &desired_state.context_mounts[0];
    assert_eq!(team_docs.remote, "https://github.com/team/docs.git");
    assert_eq!(team_docs.mount_path, "team-docs");

    let api_docs = &desired_state.context_mounts[1];
    assert_eq!(api_docs.remote, "git@github.com:company/api-docs.git");
    assert_eq!(api_docs.mount_path, "api");
    assert_eq!(api_docs.subpath, Some("docs/api".to_string()));

    // Verify references
    assert_eq!(desired_state.references.len(), 2);
    assert!(
        desired_state
            .references
            .contains(&"https://github.com/rust-lang/rust.git".to_string())
    );
    assert!(
        desired_state
            .references
            .contains(&"https://github.com/tokio-rs/tokio.git".to_string())
    );

    Ok(())
}

#[test]
fn test_v1_optional_mounts_ignored() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_dir = temp_dir.path().to_path_buf();

    // Create v1 config
    create_v1_config(&repo_dir)?;

    // Load via RepoConfigManager
    let manager = RepoConfigManager::new(repo_dir);
    let desired_state = manager.load_desired_state()?.expect("Should load config");

    // Verify the optional mount is still included (optional flag is just ignored)
    let api_docs = desired_state
        .context_mounts
        .iter()
        .find(|m| m.mount_path == "api")
        .expect("Should find api mount");

    // The mount is included regardless of optional flag
    assert_eq!(api_docs.remote, "git@github.com:company/api-docs.git");

    Ok(())
}

#[test]
fn test_references_detection_by_path() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_dir = temp_dir.path().to_path_buf();
    let config_dir = repo_dir.join(".thoughts");
    fs::create_dir_all(&config_dir)?;

    let v1_config = json!({
        "version": "1.0",
        "mount_dirs": {
            "repository": "context",
            "personal": "personal"
        },
        "requires": [
            {
                "remote": "https://github.com/rust-lang/rust.git",
                "mount_path": "references/rust",
                "description": "Rust reference",
                "sync": "auto"  // Even with auto sync, path prefix makes it a reference
            },
            {
                "remote": "https://github.com/docs/docs.git",
                "mount_path": "docs",
                "description": "Documentation",
                "sync": "auto"  // Regular context mount
            }
        ]
    });

    let config_path = config_dir.join("config.json");
    fs::write(&config_path, serde_json::to_string_pretty(&v1_config)?)?;

    let manager = RepoConfigManager::new(repo_dir);
    let desired_state = manager.load_desired_state()?.expect("Should load config");

    // Verify references detection by path
    assert_eq!(desired_state.references.len(), 1);
    assert!(
        desired_state
            .references
            .contains(&"https://github.com/rust-lang/rust.git".to_string())
    );

    // Verify context mount
    assert_eq!(desired_state.context_mounts.len(), 1);
    assert_eq!(desired_state.context_mounts[0].mount_path, "docs");

    Ok(())
}

#[test]
fn test_references_detection_by_sync_none() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_dir = temp_dir.path().to_path_buf();
    let config_dir = repo_dir.join(".thoughts");
    fs::create_dir_all(&config_dir)?;

    let v1_config = json!({
        "version": "1.0",
        "mount_dirs": {
            "repository": "context",
            "personal": "personal"
        },
        "requires": [
            {
                "remote": "https://github.com/tokio-rs/tokio.git",
                "mount_path": "tokio",  // No references/ prefix
                "description": "Tokio reference",
                "sync": "none"  // But sync:none makes it a reference
            }
        ]
    });

    let config_path = config_dir.join("config.json");
    fs::write(&config_path, serde_json::to_string_pretty(&v1_config)?)?;

    let manager = RepoConfigManager::new(repo_dir);
    let desired_state = manager.load_desired_state()?.expect("Should load config");

    // Verify references detection by sync:none
    assert_eq!(desired_state.references.len(), 1);
    assert!(
        desired_state
            .references
            .contains(&"https://github.com/tokio-rs/tokio.git".to_string())
    );
    assert_eq!(desired_state.context_mounts.len(), 0);

    Ok(())
}

#[test]
fn test_mount_space_compatibility_with_v1_targets() -> Result<()> {
    // Test that MountSpace can parse v1-style mount paths

    // Context mount
    let context = MountSpace::parse("team-docs");
    assert!(matches!(context, Ok(MountSpace::Context(path)) if path == "team-docs"));

    // Reference mount with full path
    let reference = MountSpace::parse("references/rust-lang/rust");
    assert!(matches!(
        reference,
        Ok(MountSpace::Reference { org, repo }) if org == "rust-lang" && repo == "rust"
    ));

    // Thoughts mount
    let thoughts = MountSpace::parse("thoughts");
    assert!(matches!(thoughts, Ok(MountSpace::Thoughts)));

    Ok(())
}

#[test]
fn test_v2_config_loads_correctly() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let repo_dir = temp_dir.path().to_path_buf();
    let config_dir = repo_dir.join(".thoughts");
    fs::create_dir_all(&config_dir)?;

    let v2_config = json!({
        "version": "2.0",
        "mount_dirs": {
            "thoughts": "thoughts",
            "context": "context",
            "references": "references"
        },
        "thoughts_mount": {
            "remote": "git@github.com:user/my-thoughts.git",
            "sync": "auto"
        },
        "context_mounts": [
            {
                "remote": "https://github.com/team/docs.git",
                "mount_path": "team-docs",
                "sync": "auto"
            }
        ],
        "references": [
            "https://github.com/rust-lang/rust.git"
        ]
    });

    let config_path = config_dir.join("config.json");
    fs::write(&config_path, serde_json::to_string_pretty(&v2_config)?)?;

    let manager = RepoConfigManager::new(repo_dir);
    let desired_state = manager.load_desired_state()?.expect("Should load config");

    // Verify it was NOT detected as v1
    assert!(!desired_state.was_v1, "Should not detect as v1 config");

    // Verify thoughts mount
    assert!(desired_state.thoughts_mount.is_some());
    let thoughts = desired_state.thoughts_mount.as_ref().unwrap();
    assert_eq!(thoughts.remote, "git@github.com:user/my-thoughts.git");

    // Verify context mount
    assert_eq!(desired_state.context_mounts.len(), 1);

    // Verify references
    assert_eq!(desired_state.references.len(), 1);

    Ok(())
}

#[test]
fn test_personal_config_warning_path() -> Result<()> {
    use thoughts_tool::utils::paths::get_personal_config_path;

    // Just verify the function exists and returns a path
    let path = get_personal_config_path()?;
    assert!(path.to_string_lossy().contains(".thoughts"));
    assert!(path.to_string_lossy().contains("config.json"));

    Ok(())
}
