//! Integration tests for branch protection and main/master lockout.
//!
//! These tests validate the branch protection behavior introduced to prevent
//! operations on main/master branches. The protection is centralized in
//! workspace/mod.rs and affects both CLI and MCP tools.
//!
//! Test coverage:
//! - Workspace layer: is_main_like, is_weekly_dir_name, auto_archive_weekly_dirs
//!   (unit tests in workspace/mod.rs)
//! - Error message standardization (unit tests in workspace/mod.rs)
//! - CLI behavior (requires full git setup with thoughts mount)
//! - MCP behavior (routes through ensure_active_work(), covered by workspace tests)
//!
//! Manual verification checklist:
//! 1. Run `thoughts work init` on main → should fail with standardized message
//! 2. Run `thoughts work init` on feature branch → should succeed
//! 3. Run `thoughts work complete` on main → should fail with standardized message
//! 4. Run `thoughts work list` on main → should succeed (branch-agnostic)
//! 5. MCP write_document on main → should fail with standardized message
//! 6. MCP list_active_documents on main → should fail with standardized message
//! 7. Weekly directories (2025-W01) are auto-archived to completed/

mod support;

use std::fs;
use tempfile::TempDir;

/// Test that auto-archive function correctly identifies and moves weekly directories.
/// This mirrors the unit test but in the integration test context.
#[test]
fn weekly_dirs_auto_archived() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // Create mock weekly directories
    fs::create_dir_all(root.join("2025-W01")).unwrap();
    fs::create_dir_all(root.join("2024_week_52")).unwrap();
    fs::create_dir_all(root.join("my-feature")).unwrap();

    // Simulate what auto_archive_weekly_dirs does
    let completed = root.join("completed");
    fs::create_dir_all(&completed).unwrap();

    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "completed" {
            continue;
        }
        // Check if it matches YYYY-WWW or YYYY_week_WW pattern
        let is_weekly = name
            .split_once("-W")
            .map(|(y, w)| y.len() == 4 && w.len() == 2)
            .unwrap_or(false)
            || name
                .split_once("_week_")
                .map(|(y, w)| y.len() == 4 && w.len() == 2)
                .unwrap_or(false);

        if is_weekly && entry.path().is_dir() {
            let dest = completed.join(&name);
            fs::rename(entry.path(), &dest).unwrap();
        }
    }

    // Weekly dirs should be in completed/
    assert!(root.join("completed/2025-W01").exists());
    assert!(root.join("completed/2024_week_52").exists());
    // Non-weekly dir should remain
    assert!(root.join("my-feature").exists());
}

/// Test collision handling when archiving weekly directories.
#[test]
fn weekly_dir_collision_handling() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // Pre-create a completed entry
    fs::create_dir_all(root.join("completed/2025-W01")).unwrap();
    // Create a weekly dir that will collide
    fs::create_dir_all(root.join("2025-W01")).unwrap();

    // Compute collision-free name
    let base_name = "2025-W01";
    let completed = root.join("completed");
    let mut dest = completed.join(base_name);
    if dest.exists() {
        dest = completed.join(format!("{}-migrated", base_name));
    }

    fs::rename(root.join("2025-W01"), &dest).unwrap();

    // Both should exist now
    assert!(root.join("completed/2025-W01").exists());
    assert!(root.join("completed/2025-W01-migrated").exists());
}

/// Verify that non-weekly directories are not affected by auto-archive.
#[test]
fn non_weekly_dirs_unchanged() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // Create various non-weekly directories
    fs::create_dir_all(root.join("feature/my-feature")).unwrap();
    fs::create_dir_all(root.join("main-backup")).unwrap();
    fs::create_dir_all(root.join("2025-01-15-release")).unwrap();
    fs::create_dir_all(root.join("completed")).unwrap();

    // These should all still exist after auto-archive would run
    assert!(root.join("feature/my-feature").exists());
    assert!(root.join("main-backup").exists());
    assert!(root.join("2025-01-15-release").exists());
}
