#![cfg(test)]

use tempfile::TempDir;
use thoughts_tool::{RepoConfig, RepoConfigManager, RequiredMount, SyncStrategy};

#[test]
fn test_repo_config_round_trip() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize .thoughts directory structure (configs go in .thoughts, not .thoughts-data)
    let thoughts_dir = temp_dir.path().join(".thoughts");
    std::fs::create_dir_all(&thoughts_dir).unwrap();

    let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

    // Create a RepoConfig with required mounts
    let config = RepoConfig {
        version: "1.0".to_string(),
        mount_dirs: Default::default(),
        requires: vec![RequiredMount {
            remote: "git@github.com:test/repo.git".to_string(),
            mount_path: "test_mount".to_string(),
            subpath: None,
            description: "Test mount".to_string(),
            optional: false,
            override_rules: None,
            sync: SyncStrategy::Auto,
        }],
        rules: vec![],
    };

    // Save and reload
    manager.save(&config).unwrap();
    let loaded = manager
        .load()
        .expect("Failed to load config")
        .expect("Config file should exist after save");

    // Verify
    assert_eq!(loaded.version, config.version);
    assert_eq!(loaded.requires.len(), 1);
    assert_eq!(loaded.requires[0].mount_path, "test_mount");
    assert_eq!(loaded.requires[0].remote, "git@github.com:test/repo.git");
}

// Collections removed - tests removed during refactoring

// Schema validation tests are in src/config/schema.rs::tests

// Mount validation tests are in src/config/validation.rs::tests
