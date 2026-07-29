#![expect(
    clippy::unwrap_used,
    reason = "tests should fail immediately on fixture errors"
)]

use agentic_config::types::CliToolsConfig;
use agentic_config::types::SubagentsConfig;
use coding_agent_tools::CodingAgentTools;
use coding_agent_tools::types::Depth;

#[tokio::test]
async fn sandbox_accepts_secondary_root_and_rejects_outside_paths() {
    let primary = tempfile::TempDir::new().unwrap();
    let secondary = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(secondary.path().join("allowed.txt"), "nonce").unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    let roots = vec![
        std::fs::canonicalize(primary.path()).unwrap(),
        std::fs::canonicalize(secondary.path()).unwrap(),
    ];
    let tools = CodingAgentTools::with_config_and_roots(
        SubagentsConfig::default(),
        CliToolsConfig::default(),
        Some(roots),
    );

    assert!(
        tools
            .ls(
                Some(secondary.path().display().to_string()),
                Some(Depth::new(1).unwrap()),
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .entries
            .iter()
            .any(|entry| entry.path == "allowed.txt")
    );
    assert!(
        tools
            .ls(
                Some(outside.path().display().to_string()),
                None,
                None,
                None,
                None,
            )
            .await
            .is_err()
    );
    assert!(
        tools
            .ls(Some("../escape".to_string()), None, None, None, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn sandbox_relative_paths_always_resolve_under_first_worktree_root() {
    let parent = tempfile::TempDir::new().unwrap();
    let worktree = parent.path().join("z-worktree");
    let mount = parent.path().join("a-mount");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::create_dir_all(&mount).unwrap();
    std::fs::write(worktree.join("relative.txt"), "worktree").unwrap();
    std::fs::write(mount.join("relative.txt"), "mount").unwrap();
    let tools = CodingAgentTools::with_config_and_roots(
        SubagentsConfig::default(),
        CliToolsConfig::default(),
        Some(vec![
            std::fs::canonicalize(&worktree).unwrap(),
            std::fs::canonicalize(&mount).unwrap(),
        ]),
    );

    let output = tools
        .ls(Some(".".to_string()), None, None, None, None)
        .await
        .unwrap();
    assert_eq!(
        output.root,
        std::fs::canonicalize(&worktree)
            .unwrap()
            .display()
            .to_string()
    );
    assert!(
        output
            .entries
            .iter()
            .any(|entry| entry.path == "relative.txt")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn sandbox_never_returns_symlink_escape_name() {
    use std::os::unix::fs::symlink;

    let primary = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    symlink(
        outside.path().join("secret.txt"),
        primary.path().join("leak.txt"),
    )
    .unwrap();
    let tools = CodingAgentTools::with_config_and_roots(
        SubagentsConfig::default(),
        CliToolsConfig::default(),
        Some(vec![std::fs::canonicalize(primary.path()).unwrap()]),
    );
    let output = tools.ls(None, None, None, None, None).await.unwrap();
    assert!(!output.entries.iter().any(|entry| entry.path == "leak.txt"));
}

#[tokio::test]
async fn unset_sandbox_preserves_absolute_parent_behavior() {
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("visible.txt"), "visible").unwrap();
    let tools = CodingAgentTools::new();
    let output = tools
        .ls(
            Some(outside.path().display().to_string()),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(
        output
            .entries
            .iter()
            .any(|entry| entry.path == "visible.txt")
    );
}
