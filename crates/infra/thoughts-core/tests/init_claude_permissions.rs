use std::fs;
use tempfile::TempDir;

#[cfg(unix)]
#[test]
fn worktree_additional_directory_and_relative_rules_are_injected() {
    use thoughts_tool::utils::claude_settings::inject_additional_directories;

    let td = TempDir::new().unwrap();
    let main_repo = td.path().join("main");
    let worktree = td.path().join("worktree");

    // Create main repo structure with .thoughts-data
    fs::create_dir_all(main_repo.join(".thoughts-data")).unwrap();

    // Create worktree
    fs::create_dir_all(&worktree).unwrap();

    // Create symlink: worktree/.thoughts-data -> main/.thoughts-data
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(
            main_repo.join(".thoughts-data"),
            worktree.join(".thoughts-data"),
        )
        .unwrap();
    }

    // Inject permissions
    let summary = inject_additional_directories(&worktree).unwrap();

    assert!(summary.settings_path.exists());

    let content = fs::read_to_string(&summary.settings_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Verify additionalDirectories includes canonical main repo .thoughts-data
    let add_dirs = json["permissions"]["additionalDirectories"]
        .as_array()
        .expect("additionalDirectories must be an array");

    let add_dirs_strs: Vec<&str> = add_dirs.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(add_dirs_strs.len(), 1);
    assert!(add_dirs_strs[0].contains("/main/.thoughts-data"));

    // Verify allow contains three relative rules
    let allow = json["permissions"]["allow"].as_array().unwrap();
    let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
    assert!(allow_strs.contains(&"Read(thoughts/**)"));
    assert!(allow_strs.contains(&"Read(context/**)"));
    assert!(allow_strs.contains(&"Read(references/**)"));
}

#[test]
fn regular_repo_injection_writes_additional_directories_and_rules() {
    use thoughts_tool::utils::claude_settings::inject_additional_directories;

    let td = TempDir::new().unwrap();
    let repo = td.path();

    // Local .thoughts-data should exist for canonicalization
    fs::create_dir_all(repo.join(".thoughts-data")).unwrap();

    let summary = inject_additional_directories(repo).unwrap();
    assert!(
        summary
            .settings_path
            .ends_with(".claude/settings.local.json")
    );

    let content = fs::read_to_string(&summary.settings_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Verify structure
    assert!(json["permissions"]["additionalDirectories"].is_array());
    assert!(json["permissions"]["allow"].is_array());
    assert!(json["permissions"]["deny"].is_array());
    assert!(json["permissions"]["ask"].is_array());

    // Verify allow rules are present
    let allow = json["permissions"]["allow"].as_array().unwrap();
    let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
    assert!(allow_strs.contains(&"Read(thoughts/**)"));
    assert!(allow_strs.contains(&"Read(context/**)"));
    assert!(allow_strs.contains(&"Read(references/**)"));
}

#[test]
fn idempotent_multiple_runs_do_not_duplicate() {
    use thoughts_tool::utils::claude_settings::inject_additional_directories;

    let td = TempDir::new().unwrap();
    let repo = td.path();

    fs::create_dir_all(repo.join(".thoughts-data")).unwrap();

    // First run
    let first = inject_additional_directories(repo).unwrap();
    let new_first = first.added_additional_dirs.len() + first.added_allow_rules.len();
    assert!(new_first > 0);

    // Second run
    let second = inject_additional_directories(repo).unwrap();
    let new_second = second.added_additional_dirs.len() + second.added_allow_rules.len();
    assert_eq!(new_second, 0);

    // Verify no duplicates in file
    let content = fs::read_to_string(&first.settings_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    let allow = json["permissions"]["allow"].as_array().unwrap();

    let mut seen = std::collections::HashSet::new();
    for item in allow {
        if let Some(s) = item.as_str() {
            assert!(seen.insert(s.to_string()), "Found duplicate: {}", s);
        }
    }
}
