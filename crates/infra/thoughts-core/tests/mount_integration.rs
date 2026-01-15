#![cfg(test)]

use tempfile::TempDir;
use thoughts_tool::{MountOptions, detect_platform, get_mount_manager};

#[tokio::test]
#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), ignore)]
async fn test_basic_mount_unmount() {
    // This test requires appropriate permissions and tools installed
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").is_err() {
        eprintln!("Skipping integration test. Set THOUGHTS_INTEGRATION_TESTS=1 to run.");
        return;
    }

    let platform_info = detect_platform().expect("Failed to detect platform");

    // Check if platform can mount using the Platform enum's method
    if !platform_info.platform.can_mount() {
        eprintln!("Skipping test: mount tools not available");
        if let Some(tool_name) = platform_info.platform.mount_tool_name() {
            eprintln!("Required tool: {}", tool_name);
        }
        return;
    }

    // Create test directories
    let temp_dir = TempDir::new().unwrap();
    let source1 = temp_dir.path().join("source1");
    let source2 = temp_dir.path().join("source2");
    let target = temp_dir.path().join("merged");

    std::fs::create_dir_all(&source1).unwrap();
    std::fs::create_dir_all(&source2).unwrap();

    // Create test files
    std::fs::write(source1.join("file1.txt"), "content1").unwrap();
    std::fs::write(source2.join("file2.txt"), "content2").unwrap();

    // Get mount manager
    let manager = get_mount_manager(&platform_info).expect("Failed to create mount manager");

    // Test mount
    let options = MountOptions::default();
    let sources = vec![source1.clone(), source2.clone()];

    manager
        .mount(&sources, &target, &options)
        .await
        .expect("Mount failed");

    // Verify mount
    assert!(manager.is_mounted(&target).await.unwrap());

    // Verify merged content
    assert!(target.join("file1.txt").exists());
    assert!(target.join("file2.txt").exists());

    // Test unmount
    manager
        .unmount(&target, false)
        .await
        .expect("Unmount failed");

    // Verify unmount
    assert!(!manager.is_mounted(&target).await.unwrap());
}

/// Test that remounting the same target is idempotent (no duplicates)
/// This is a regression test for GitHub Issue #19 where FUSE-T verification
/// failures caused duplicate mounts due to retry logic
#[tokio::test]
#[cfg(target_os = "macos")]
async fn test_remount_is_idempotent() {
    // This test requires FUSE-T/macFUSE + unionfs-fuse setup
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").is_err() {
        eprintln!("Skipping integration test. Set THOUGHTS_INTEGRATION_TESTS=1 to run.");
        return;
    }

    let platform_info = detect_platform().expect("Failed to detect platform");

    if !platform_info.platform.can_mount() {
        eprintln!("Skipping test: mount tools not available");
        if let Some(tool_name) = platform_info.platform.mount_tool_name() {
            eprintln!("Required tool: {}", tool_name);
        }
        return;
    }

    // Create test directories
    let temp_dir = TempDir::new().unwrap();
    let source1 = temp_dir.path().join("source1");
    let source2 = temp_dir.path().join("source2");
    let target = temp_dir.path().join("merged");

    std::fs::create_dir_all(&source1).unwrap();
    std::fs::create_dir_all(&source2).unwrap();

    // Create test files
    std::fs::write(source1.join("file1.txt"), "content1").unwrap();
    std::fs::write(source2.join("file2.txt"), "content2").unwrap();

    let manager = get_mount_manager(&platform_info).expect("Failed to create mount manager");
    let options = MountOptions::default();
    let sources = vec![source1.clone(), source2.clone()];

    // First mount
    manager
        .mount(&sources, &target, &options)
        .await
        .expect("First mount failed");

    assert!(
        manager.is_mounted(&target).await.unwrap(),
        "First mount should be detected"
    );

    // Second mount attempt should be idempotent (no-op, no duplicates)
    manager
        .mount(&sources, &target, &options)
        .await
        .expect("Second mount failed");

    assert!(
        manager.is_mounted(&target).await.unwrap(),
        "Mount should still be detected"
    );

    // Verify only one mount entry exists for this target
    let out = std::process::Command::new("mount")
        .output()
        .expect("Failed to run mount command");
    let text = String::from_utf8_lossy(&out.stdout);
    let target_str = target.display().to_string();
    let count = text
        .lines()
        .filter(|l| l.contains(" on ") && l.contains(&target_str))
        .count();

    assert_eq!(
        count, 1,
        "There should be exactly one mount entry for the target. Found {} entries. \
         This indicates duplicate mounts were created (Issue #19 regression).",
        count
    );

    // Cleanup
    manager
        .unmount(&target, false)
        .await
        .expect("Unmount failed");

    assert!(
        !manager.is_mounted(&target).await.unwrap(),
        "Mount should be cleaned up"
    );
}
