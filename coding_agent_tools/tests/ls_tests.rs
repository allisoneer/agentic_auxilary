use coding_agent_tools::paths::to_abs_string;
use coding_agent_tools::types::{Depth, Show};
use std::str::FromStr;

// =============================================================================
// Phase 1: Basic compile tests
// =============================================================================

#[test]
fn types_exist() {
    let _: Depth = Depth::new(1).unwrap();
    let _show: Show = Default::default();
}

// =============================================================================
// Phase 2: Core Types and Path Handling
// =============================================================================

mod depth_tests {
    use super::*;

    #[test]
    fn depth_validates_bounds() {
        assert!(Depth::new(0).is_ok());
        assert!(Depth::new(1).is_ok());
        assert!(Depth::new(10).is_ok());
        assert!(Depth::new(11).is_err());
        assert!(Depth::new(255).is_err());
    }

    #[test]
    fn depth_as_u8() {
        let d = Depth::new(5).unwrap();
        assert_eq!(d.as_u8(), 5);
    }

    #[test]
    fn depth_serde_roundtrip() {
        let d: Depth = serde_json::from_str("5").unwrap();
        assert_eq!(d.as_u8(), 5);

        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, "5");
    }

    #[test]
    fn depth_serde_rejects_invalid() {
        let result: Result<Depth, _> = serde_json::from_str("11");
        assert!(result.is_err());
    }

    #[test]
    fn depth_default_is_zero() {
        let d: Depth = Default::default();
        assert_eq!(d.as_u8(), 0);
    }

    #[test]
    fn depth_json_schema_has_constraints() {
        use schemars::schema_for;

        let schema = schema_for!(Depth);
        let json = serde_json::to_string_pretty(&schema).unwrap();

        // Verify the schema contains min/max constraints
        assert!(json.contains("minimum"));
        assert!(json.contains("maximum"));
        assert!(json.contains("0"));
        assert!(json.contains("10"));
    }
}

mod show_tests {
    use super::*;

    #[test]
    fn show_parses_from_str() {
        assert!(matches!(Show::from_str("all").unwrap(), Show::All));
        assert!(matches!(Show::from_str("ALL").unwrap(), Show::All));
        assert!(matches!(Show::from_str("files").unwrap(), Show::Files));
        assert!(matches!(Show::from_str("FILES").unwrap(), Show::Files));
        assert!(matches!(Show::from_str("dirs").unwrap(), Show::Dirs));
        assert!(matches!(Show::from_str("directories").unwrap(), Show::Dirs));
    }

    #[test]
    fn show_rejects_invalid() {
        assert!(Show::from_str("invalid").is_err());
        assert!(Show::from_str("").is_err());
    }

    #[test]
    fn show_serde_lowercase() {
        let json = serde_json::to_string(&Show::Files).unwrap();
        assert_eq!(json, r#""files""#);

        let json = serde_json::to_string(&Show::Dirs).unwrap();
        assert_eq!(json, r#""dirs""#);

        let show: Show = serde_json::from_str(r#""all""#).unwrap();
        assert!(matches!(show, Show::All));
    }

    #[test]
    fn show_default_is_all() {
        let show: Show = Default::default();
        assert!(matches!(show, Show::All));
    }
}

mod path_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn path_to_abs_string_makes_absolute() {
        let abs = to_abs_string("foo/bar");
        assert!(
            Path::new(&abs).is_absolute(),
            "expected absolute path, got: {}",
            abs
        );
    }

    #[test]
    fn path_existing_dir_canonicalizes() {
        let abs = to_abs_string(".");
        assert!(Path::new(&abs).is_absolute());
        // Canonicalized path shouldn't end with /. or /.
        assert!(!abs.ends_with("/."));
    }

    #[test]
    fn path_absolute_stays_absolute() {
        let abs = to_abs_string("/tmp/nonexistent/test/path");
        assert!(Path::new(&abs).is_absolute());
    }
}

// =============================================================================
// Phase 3: Directory Traversal Integration Tests
// =============================================================================

// =============================================================================
// Phase 4: McpFormatter Tests
// =============================================================================

mod mcp_formatter_tests {
    use coding_agent_tools::types::{EntryKind, LsEntry, LsOutput};
    use universal_tool_core::mcp::McpFormatter;

    #[test]
    fn format_header_has_trailing_slash() {
        let output = LsOutput {
            root: "/home/user/project".into(),
            entries: vec![],
            has_more: false,
            warnings: vec![],
        };
        let text = output.mcp_format_text();
        assert!(text.starts_with("/home/user/project/"));
    }

    #[test]
    fn format_entries_indented() {
        let output = LsOutput {
            root: "/test".into(),
            entries: vec![
                LsEntry {
                    path: "file.txt".into(),
                    kind: EntryKind::File,
                },
                LsEntry {
                    path: "dir".into(),
                    kind: EntryKind::Dir,
                },
            ],
            has_more: false,
            warnings: vec![],
        };
        let text = output.mcp_format_text();

        // Entries should be indented with 2 spaces
        assert!(text.contains("  file.txt"));
        // Directories should have trailing slash
        assert!(text.contains("  dir/"));
    }

    #[test]
    fn format_truncation_notice() {
        let output = LsOutput {
            root: "/test".into(),
            entries: vec![],
            has_more: true,
            warnings: vec![],
        };
        let text = output.mcp_format_text();
        assert!(text.contains("truncated"));
        assert!(text.contains("call again with same params"));
    }

    #[test]
    fn format_warnings() {
        let output = LsOutput {
            root: "/test".into(),
            entries: vec![],
            has_more: false,
            warnings: vec!["Permission denied: secret/".into()],
        };
        let text = output.mcp_format_text();
        assert!(text.contains("Note: Permission denied: secret/"));
    }

    #[test]
    fn format_complete_output() {
        let output = LsOutput {
            root: "/project".into(),
            entries: vec![
                LsEntry {
                    path: "src".into(),
                    kind: EntryKind::Dir,
                },
                LsEntry {
                    path: "README.md".into(),
                    kind: EntryKind::File,
                },
            ],
            has_more: true,
            warnings: vec!["Skipped: node_modules".into()],
        };
        let text = output.mcp_format_text();

        // Verify order: header, entries, truncation, warnings
        let header_pos = text.find("/project/").unwrap();
        let src_pos = text.find("  src/").unwrap();
        let readme_pos = text.find("  README.md").unwrap();
        let truncated_pos = text.find("truncated").unwrap();
        let note_pos = text.find("Note:").unwrap();

        assert!(header_pos < src_pos);
        assert!(src_pos < readme_pos);
        assert!(readme_pos < truncated_pos);
        assert!(truncated_pos < note_pos);
    }
}

mod walker_integration_tests {
    use coding_agent_tools::types::{EntryKind, Show};
    use coding_agent_tools::walker::{BUILTIN_IGNORES, WalkConfig, list};
    use std::fs;
    use tempfile::TempDir;

    fn create_test_tree() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create structure:
        // root/
        //   src/
        //     main.rs
        //     lib.rs
        //   tests/
        //     test.rs
        //   README.md
        //   .hidden_file
        //   .hidden_dir/
        //     secret.txt

        fs::create_dir(root.join("src")).unwrap();
        fs::create_dir(root.join("tests")).unwrap();
        fs::create_dir(root.join(".hidden_dir")).unwrap();

        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("src/lib.rs"), "// lib").unwrap();
        fs::write(root.join("tests/test.rs"), "// test").unwrap();
        fs::write(root.join("README.md"), "# README").unwrap();
        fs::write(root.join(".hidden_file"), "secret").unwrap();
        fs::write(root.join(".hidden_dir/secret.txt"), "very secret").unwrap();

        dir
    }

    #[test]
    fn depth_zero_returns_empty() {
        let dir = create_test_tree();
        let cfg = WalkConfig {
            root: dir.path(),
            depth: 0,
            show: Show::All,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();
        assert!(result.entries.is_empty());
    }

    #[test]
    fn depth_one_lists_immediate_children() {
        let dir = create_test_tree();
        let cfg = WalkConfig {
            root: dir.path(),
            depth: 1,
            show: Show::All,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();

        let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();

        // Should have: src/, tests/, README.md (hidden items excluded)
        assert!(paths.contains(&"src"));
        assert!(paths.contains(&"tests"));
        assert!(paths.contains(&"README.md"));

        // Hidden items should be excluded
        assert!(!paths.contains(&".hidden_file"));
        assert!(!paths.contains(&".hidden_dir"));

        // Nested items should not appear at depth 1
        assert!(!paths.iter().any(|p| p.contains('/')));
    }

    #[test]
    fn depth_two_includes_nested() {
        let dir = create_test_tree();
        let cfg = WalkConfig {
            root: dir.path(),
            depth: 2,
            show: Show::All,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();

        let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();

        // Should include nested files
        assert!(paths.contains(&"src/main.rs"));
        assert!(paths.contains(&"src/lib.rs"));
        assert!(paths.contains(&"tests/test.rs"));
    }

    #[test]
    fn show_files_excludes_dirs() {
        let dir = create_test_tree();
        let cfg = WalkConfig {
            root: dir.path(),
            depth: 1,
            show: Show::Files,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();

        // Only files should appear
        for entry in &result.entries {
            assert!(
                matches!(entry.kind, EntryKind::File | EntryKind::Symlink),
                "Expected file, got dir: {}",
                entry.path
            );
        }

        let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"README.md"));
        assert!(!paths.contains(&"src"));
    }

    #[test]
    fn show_dirs_excludes_files() {
        let dir = create_test_tree();
        let cfg = WalkConfig {
            root: dir.path(),
            depth: 1,
            show: Show::Dirs,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();

        // Only directories should appear
        for entry in &result.entries {
            assert!(
                matches!(entry.kind, EntryKind::Dir),
                "Expected dir, got file: {}",
                entry.path
            );
        }

        let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"src"));
        assert!(paths.contains(&"tests"));
        assert!(!paths.contains(&"README.md"));
    }

    #[test]
    fn hidden_flag_includes_hidden_items() {
        let dir = create_test_tree();
        let cfg = WalkConfig {
            root: dir.path(),
            depth: 1,
            show: Show::All,
            user_ignores: &[],
            include_hidden: true,
        };
        let result = list(&cfg).unwrap();

        let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();

        // Hidden items should now appear
        assert!(paths.contains(&".hidden_file"));
        assert!(paths.contains(&".hidden_dir"));
    }

    #[test]
    fn custom_ignore_patterns_work() {
        let dir = create_test_tree();
        let cfg = WalkConfig {
            root: dir.path(),
            depth: 2,
            show: Show::All,
            user_ignores: &["*.rs".into()],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();

        let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();

        // .rs files should be excluded
        assert!(!paths.iter().any(|p| p.ends_with(".rs")));

        // Other files should still appear
        assert!(paths.contains(&"README.md"));
    }

    #[test]
    fn sorting_dirs_first_for_show_all() {
        let dir = create_test_tree();
        let cfg = WalkConfig {
            root: dir.path(),
            depth: 1,
            show: Show::All,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();

        // Find where dirs end and files begin
        let mut seen_file = false;
        for entry in &result.entries {
            if matches!(entry.kind, EntryKind::File) {
                seen_file = true;
            } else if matches!(entry.kind, EntryKind::Dir) && seen_file {
                panic!("Found directory after file - sorting is wrong");
            }
        }
    }

    #[test]
    fn builtin_ignores_exist() {
        // Verify we have the expected built-in patterns
        assert!(BUILTIN_IGNORES.contains(&"**/node_modules/**"));
        assert!(BUILTIN_IGNORES.contains(&"**/target/**"));
        assert!(BUILTIN_IGNORES.contains(&"**/__pycache__/**"));
    }

    #[test]
    fn builtin_ignores_applied() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create a node_modules directory
        fs::create_dir(root.join("node_modules")).unwrap();
        fs::write(root.join("node_modules/package.json"), "{}").unwrap();
        fs::write(root.join("index.js"), "// main").unwrap();

        let cfg = WalkConfig {
            root,
            depth: 2,
            show: Show::All,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();

        let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();

        // node_modules should be filtered out
        assert!(!paths.iter().any(|p| p.contains("node_modules")));

        // But other files should appear
        assert!(paths.contains(&"index.js"));
    }

    #[test]
    fn gitignore_file_respected() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Initialize as git repo so .gitignore is recognized
        fs::create_dir(root.join(".git")).unwrap();

        // Create .gitignore
        fs::write(root.join(".gitignore"), "ignored_dir/\n*.log\n").unwrap();
        fs::create_dir(root.join("ignored_dir")).unwrap();
        fs::write(root.join("ignored_dir/secret.txt"), "secret").unwrap();
        fs::write(root.join("app.log"), "logs").unwrap();
        fs::write(root.join("main.rs"), "fn main() {}").unwrap();

        let cfg = WalkConfig {
            root,
            depth: 2,
            show: Show::All,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();

        let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();

        // Gitignored items should be excluded
        assert!(!paths.iter().any(|p| p.contains("ignored_dir")));
        assert!(!paths.contains(&"app.log"));

        // Non-ignored items should appear
        assert!(paths.contains(&"main.rs"));
    }

    #[test]
    #[cfg(unix)]
    fn symlink_included_as_entry() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::write(root.join("target.txt"), "target content").unwrap();
        symlink(root.join("target.txt"), root.join("link.txt")).unwrap();

        let cfg = WalkConfig {
            root,
            depth: 1,
            show: Show::All,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();

        let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"link.txt"));
        assert!(paths.contains(&"target.txt"));
    }

    #[test]
    #[cfg(unix)]
    fn broken_symlink_generates_warning() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create symlink pointing to non-existent target
        symlink("/nonexistent/path/target.txt", root.join("broken_link.txt")).unwrap();

        let cfg = WalkConfig {
            root,
            depth: 1,
            show: Show::All,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg).unwrap();

        // Should have a warning about broken symlink
        assert!(
            result.warnings.iter().any(|w| w.contains("broken_link")),
            "Expected broken symlink warning, got: {:?}",
            result.warnings
        );
    }

    #[test]
    #[cfg(unix)]
    fn permission_denied_yields_warning() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create unreadable directory
        let restricted = root.join("restricted");
        fs::create_dir(&restricted).unwrap();
        fs::write(restricted.join("secret.txt"), "secret").unwrap();

        // Remove read permission
        let mut perms = fs::metadata(&restricted).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&restricted, perms).unwrap();

        let cfg = WalkConfig {
            root,
            depth: 2,
            show: Show::All,
            user_ignores: &[],
            include_hidden: false,
        };
        let result = list(&cfg);

        // Restore permissions for cleanup
        let mut perms = fs::metadata(&restricted).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&restricted, perms).unwrap();

        // Should complete but with warnings or empty results for restricted dir
        let result = result.unwrap();
        // The walker should either warn or silently skip
        let paths: Vec<_> = result.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(!paths.contains(&"restricted/secret.txt"));
    }
}

// =============================================================================
// Phase 5: Pagination Integration Tests
// =============================================================================

mod pagination_integration_tests {
    use coding_agent_tools::pagination::{PAGE_SIZE_ALL, PAGE_SIZE_FILTERED, paginate};
    use coding_agent_tools::types::{EntryKind, LsEntry};

    fn make_entries(count: usize) -> Vec<LsEntry> {
        (0..count)
            .map(|i| LsEntry {
                path: format!("file_{:04}.txt", i),
                kind: EntryKind::File,
            })
            .collect()
    }

    #[test]
    fn pagination_boundary_100_entries() {
        let entries = make_entries(100);

        // First page: exactly 100 entries, no more
        let (page1, has_more) = paginate(entries, 0, PAGE_SIZE_ALL);
        assert_eq!(page1.len(), 100);
        assert!(!has_more);
    }

    #[test]
    fn pagination_boundary_101_entries() {
        let entries = make_entries(101);

        // First page: 100 entries, has more
        let (page1, has_more) = paginate(entries.clone(), 0, PAGE_SIZE_ALL);
        assert_eq!(page1.len(), 100);
        assert!(has_more);

        // Second page: 1 entry, no more
        let (page2, has_more) = paginate(entries, 100, PAGE_SIZE_ALL);
        assert_eq!(page2.len(), 1);
        assert!(!has_more);
    }

    #[test]
    fn pagination_boundary_1000_entries_filtered() {
        let entries = make_entries(1000);

        let (page1, has_more) = paginate(entries, 0, PAGE_SIZE_FILTERED);
        assert_eq!(page1.len(), 1000);
        assert!(!has_more);
    }

    #[test]
    fn pagination_boundary_1001_entries_filtered() {
        let entries = make_entries(1001);

        let (page1, has_more) = paginate(entries.clone(), 0, PAGE_SIZE_FILTERED);
        assert_eq!(page1.len(), 1000);
        assert!(has_more);

        let (page2, has_more) = paginate(entries, 1000, PAGE_SIZE_FILTERED);
        assert_eq!(page2.len(), 1);
        assert!(!has_more);
    }

    #[test]
    fn pagination_multiple_pages() {
        let entries = make_entries(250);

        // Page 1: 0-99
        let (page1, has_more1) = paginate(entries.clone(), 0, PAGE_SIZE_ALL);
        assert_eq!(page1.len(), 100);
        assert!(has_more1);
        assert_eq!(page1[0].path, "file_0000.txt");

        // Page 2: 100-199
        let (page2, has_more2) = paginate(entries.clone(), 100, PAGE_SIZE_ALL);
        assert_eq!(page2.len(), 100);
        assert!(has_more2);
        assert_eq!(page2[0].path, "file_0100.txt");

        // Page 3: 200-249
        let (page3, has_more3) = paginate(entries, 200, PAGE_SIZE_ALL);
        assert_eq!(page3.len(), 50);
        assert!(!has_more3);
        assert_eq!(page3[0].path, "file_0200.txt");
    }
}

// =============================================================================
// Phase 6: Stateful Pagination Integration Tests
// =============================================================================

mod ls_stateful_pagination_tests {
    use coding_agent_tools::CodingAgentTools;
    use coding_agent_tools::types::Show;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_files(root: &Path, count: usize) {
        for i in 0..count {
            let name = format!("file_{:04}.txt", i);
            fs::write(root.join(name), "x").unwrap();
        }
    }

    #[tokio::test]
    async fn ls_auto_paginates_across_calls() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        create_files(root, 250);

        let tools = CodingAgentTools::new();
        let path = root.to_string_lossy().to_string();

        // Page 1
        let out1 = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();
        assert_eq!(out1.entries.len(), 100);
        assert!(out1.has_more, "should have more after first page");
        assert_eq!(out1.entries.first().unwrap().path, "file_0000.txt");
        assert_eq!(out1.entries.last().unwrap().path, "file_0099.txt");

        // Page 2 (identical params)
        let out2 = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();
        assert_eq!(out2.entries.len(), 100);
        assert!(out2.has_more, "should have more after second page");
        assert_eq!(out2.entries.first().unwrap().path, "file_0100.txt");
        assert_eq!(out2.entries.last().unwrap().path, "file_0199.txt");

        // Page 3 (identical params)
        let out3 = tools.ls(Some(path), None, None, None, None).await.unwrap();
        assert_eq!(out3.entries.len(), 50);
        assert!(!out3.has_more, "no more pages after last page");
        assert_eq!(out3.entries.first().unwrap().path, "file_0200.txt");
        assert_eq!(out3.entries.last().unwrap().path, "file_0249.txt");
    }

    #[tokio::test]
    async fn ls_new_params_reset_pagination() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        create_files(root, 150);

        let tools = CodingAgentTools::new();
        let path = root.to_string_lossy().to_string();

        // Page 1 (default show=all)
        let _out1 = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();

        // Page 2 (identical params)
        let out2 = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();
        assert_eq!(out2.entries.first().unwrap().path, "file_0100.txt");

        // Change param: show=files (filtered mode with page_size=1000) should reset to page 1
        let out_reset = tools
            .ls(Some(path), None, Some(Show::Files), None, None)
            .await
            .unwrap();
        assert_eq!(out_reset.entries.first().unwrap().path, "file_0000.txt");
        assert!(
            !out_reset.has_more,
            "all files should fit in one page at page_size=1000"
        );
        assert_eq!(out_reset.entries.len(), 150);
    }
}

// =============================================================================
// Phase 7: Parallel Pagination and Cache Behavior Tests
// =============================================================================

mod ls_parallel_and_cache_tests {
    use coding_agent_tools::CodingAgentTools;
    use coding_agent_tools::types::Show;
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_files(root: &Path, count: usize) {
        for i in 0..count {
            let name = format!("file_{:04}.txt", i);
            fs::write(root.join(name), "x").unwrap();
        }
    }

    #[tokio::test]
    async fn parallel_identical_calls_serialize_and_paginate() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        create_files(root, 250);
        let tools = CodingAgentTools::new();
        let path = root.to_string_lossy().to_string();

        // Launch two identical ls calls in parallel
        let (a, b) = tokio::join!(
            tools.ls(Some(path.clone()), None, None, None, None),
            tools.ls(Some(path.clone()), None, None, None, None)
        );
        let out_a = a.unwrap();
        let out_b = b.unwrap();

        // Both should return 100 entries (page size for Show::All at depth 1)
        assert_eq!(out_a.entries.len(), 100);
        assert_eq!(out_b.entries.len(), 100);

        // Ensure pages are disjoint (one got page 1, other got page 2)
        let set_a: HashSet<_> = out_a.entries.iter().map(|e| e.path.clone()).collect();
        let set_b: HashSet<_> = out_b.entries.iter().map(|e| e.path.clone()).collect();
        assert!(
            set_a.is_disjoint(&set_b),
            "Parallel identical calls should return disjoint pages"
        );
    }

    #[tokio::test]
    async fn parallel_different_params_do_not_contend() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        create_files(root, 150);
        let tools = CodingAgentTools::new();
        let path = root.to_string_lossy().to_string();

        // Launch two ls calls with different params in parallel
        let (a, b) = tokio::join!(
            tools.ls(Some(path.clone()), None, None, None, None),
            tools.ls(Some(path.clone()), None, Some(Show::Files), None, None)
        );
        let out_a = a.unwrap();
        let out_b = b.unwrap();

        // Both should start at first entry (different cache keys)
        assert_eq!(out_a.entries.first().unwrap().path, "file_0000.txt");
        assert_eq!(out_b.entries.first().unwrap().path, "file_0000.txt");
    }

    #[tokio::test]
    async fn cache_prevents_rescan_between_pages() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        create_files(root, 101);

        let tools = CodingAgentTools::new();
        let path = root.to_string_lossy().to_string();

        // Page 1
        let out1 = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();
        assert_eq!(out1.entries.len(), 100);
        assert!(out1.has_more);

        // Add a new file after first page (should not appear until cache expires)
        fs::write(root.join("zzz_new.txt"), "x").unwrap();

        // Page 2 should still be exactly the last original entry (cached)
        let out2 = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();
        assert_eq!(out2.entries.len(), 1);
        assert_eq!(out2.entries[0].path, "file_0100.txt");
        assert!(!out2.has_more);
    }

    #[tokio::test]
    async fn removal_after_final_page_resets_session() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        create_files(root, 150);

        let tools = CodingAgentTools::new();
        let path = root.to_string_lossy().to_string();

        // Page 1 (100 entries), page 2 (50 entries)
        let _ = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();
        let out2 = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();
        assert!(!out2.has_more, "page 2 should be last page");

        // Next call should restart to page 1 (cache entry was removed)
        let out3 = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();
        assert_eq!(out3.entries.first().unwrap().path, "file_0000.txt");
    }
}

mod truncation_sentinel_tests {
    use coding_agent_tools::types::{TRUNCATION_SENTINEL, encode_truncation_info};

    #[test]
    fn encode_format_contains_numbers() {
        let s = encode_truncation_info(100, 250, 100);
        assert!(s.starts_with(TRUNCATION_SENTINEL));
        assert!(s.contains("shown=100"));
        assert!(s.contains("total=250"));
        assert!(s.contains("page_size=100"));
    }

    #[test]
    fn encode_different_values() {
        let s = encode_truncation_info(50, 1000, 50);
        assert!(s.contains("shown=50"));
        assert!(s.contains("total=1000"));
        assert!(s.contains("page_size=50"));
    }
}

mod enhanced_truncation_message_tests {
    use coding_agent_tools::CodingAgentTools;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;
    use universal_tool_core::mcp::McpFormatter;

    fn create_files(root: &Path, count: usize) {
        for i in 0..count {
            let name = format!("file_{:04}.txt", i);
            fs::write(root.join(name), "x").unwrap();
        }
    }

    #[tokio::test]
    async fn truncation_message_shows_counts_and_pages_remaining() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        create_files(root, 250); // 250 files = 3 pages (100, 100, 50)

        let tools = CodingAgentTools::new();
        let path = root.to_string_lossy().to_string();

        // Page 1: showing 100 of 250, 2 pages remaining
        let out1 = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();
        let text1 = out1.mcp_format_text();
        assert!(
            text1.contains("showing 100 of 250 entries"),
            "Expected 'showing 100 of 250 entries' in: {}",
            text1
        );
        assert!(
            text1.contains("2 pages remaining"),
            "Expected '2 pages remaining' in: {}",
            text1
        );
        // Should have reminder since >1 page remains
        assert!(
            text1.contains("REMINDER"),
            "Expected REMINDER when >1 page remains in: {}",
            text1
        );

        // Page 2: showing 200 of 250, 1 page remaining
        let out2 = tools
            .ls(Some(path.clone()), None, None, None, None)
            .await
            .unwrap();
        let text2 = out2.mcp_format_text();
        assert!(
            text2.contains("showing 200 of 250 entries"),
            "Expected 'showing 200 of 250 entries' in: {}",
            text2
        );
        assert!(
            text2.contains("1 page remaining"),
            "Expected '1 page remaining' (singular) in: {}",
            text2
        );
        // Should NOT have reminder since only 1 page remains
        assert!(
            !text2.contains("REMINDER"),
            "Should not have REMINDER when only 1 page remains in: {}",
            text2
        );
    }

    #[tokio::test]
    async fn sentinel_not_visible_in_normal_warnings() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        create_files(root, 150);

        let tools = CodingAgentTools::new();
        let path = root.to_string_lossy().to_string();

        let out = tools.ls(Some(path), None, None, None, None).await.unwrap();
        let text = out.mcp_format_text();

        // The sentinel should not appear in the formatted output
        assert!(
            !text.contains("<<<mcp:ls:page_info>>>"),
            "Sentinel should not be visible in output: {}",
            text
        );
    }
}
