#![cfg(test)]

use thoughts_tool::{Config, ConfigManager};
// use std::path::PathBuf;
use tempfile::TempDir;
// use serde_json::json;

#[test]
fn test_config_round_trip() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.json");
    let manager = ConfigManager::with_path(config_path);

    // Create config with mounts
    let mut config = Config::default();

    // Create directories first
    let mount_source = temp_dir.path().join("mount_source");
    std::fs::create_dir_all(&mount_source).unwrap();

    // Add mount
    config.mounts.insert(
        "test_mount".to_string(),
        thoughts_tool::Mount::Directory {
            path: mount_source,
            sync: thoughts_tool::SyncStrategy::None,
        },
    );

    // Save and reload
    manager.save(&config).unwrap();
    let loaded = manager.load().unwrap();

    // Verify
    assert_eq!(loaded.version, config.version);
    assert_eq!(loaded.mounts.len(), 1);
}

// Collections removed - tests removed during refactoring

// Schema validation tests are in src/config/schema.rs::tests

// Mount validation tests are in src/config/validation.rs::tests
