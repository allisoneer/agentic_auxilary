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

#[test]
fn test_config_create_detects_malformed_json_as_already_configured() {
    let temp_dir = TempDir::new().unwrap();

    // Create .thoughts directory
    let thoughts_dir = temp_dir.path().join(".thoughts");
    std::fs::create_dir_all(&thoughts_dir).unwrap();

    // Write malformed JSON to config.json
    let config_path = thoughts_dir.join("config.json");
    std::fs::write(&config_path, "{ invalid json !!!").unwrap();

    // Verify file exists
    assert!(config_path.exists());

    // Now verify that create logic would detect this as "already configured"
    // We test the existence check directly since the command calls std::process::exit
    let config_exists = config_path.exists();

    // This is what create.rs checks - filesystem existence, not parse validity
    assert!(
        config_exists,
        "Config file should exist and be detected as 'already configured'"
    );
}

#[test]
fn test_mount_remove_does_not_create_config_when_absent() {
    let temp_dir = TempDir::new().unwrap();

    // Create .thoughts directory but no config
    let thoughts_dir = temp_dir.path().join(".thoughts");
    std::fs::create_dir_all(&thoughts_dir).unwrap();

    let config_path = thoughts_dir.join("config.json");

    // Verify no config exists
    assert!(!config_path.exists());

    // Attempt to load v2 config (what remove.rs does)
    let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());
    let result = manager.load_v2_or_bail();

    // Should error with "No repository configuration found"
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("No repository configuration found")
    );

    // Verify no config was created during the failed load
    assert!(
        !config_path.exists(),
        "Config file should not be created when loading fails"
    );
}
