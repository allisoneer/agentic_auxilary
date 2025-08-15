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

    if !platform_info.can_mount {
        eprintln!("Skipping test: mount tools not available");
        eprintln!("Missing tools: {:?}", platform_info.missing_tools);
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
