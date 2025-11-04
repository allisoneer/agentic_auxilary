use crate::engine::paths::{is_ancestor, to_abs_string, walk_up_to_boundary};
use crate::types::{DirectoryMeta, FileMeta};

pub fn memory_files_in_dir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let claude_md = dir.join("CLAUDE.md");
    if claude_md.exists() && claude_md.is_file() {
        out.push(claude_md);
    }
    let dot_claude_md = dir.join(".claude").join("CLAUDE.md");
    if dot_claude_md.exists() && dot_claude_md.is_file() {
        out.push(dot_claude_md);
    }
    out
}

pub fn injection_enabled_from_env() -> bool {
    match std::env::var("INJECT_CLAUDE_MD") {
        Err(_) => true,
        Ok(val) => {
            let v = val.trim().to_ascii_lowercase();
            !(v == "0" || v == "false")
        }
    }
}

pub fn auto_inject_claude_memories(
    files: &mut Vec<FileMeta>,
    directories: Option<&[DirectoryMeta]>,
) -> usize {
    let cwd_raw = match std::env::current_dir() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Skipping CLAUDE.md injection: unable to get cwd: {}", e);
            return 0;
        }
    };
    let cwd = match std::fs::canonicalize(&cwd_raw) {
        Ok(canonical) => canonical,
        Err(e) => {
            tracing::warn!(
                "Unable to canonicalize cwd ({}); using raw cwd {}",
                e,
                cwd_raw.display()
            );
            cwd_raw
        }
    };

    let mut parent_dirs = Vec::<std::path::PathBuf>::new();
    let mut seen_dirs = std::collections::HashSet::<std::path::PathBuf>::new();

    // NEW: Seed from explicitly requested directories first
    if let Some(dirs) = directories {
        for d in dirs {
            let abs = to_abs_string(&d.directory_path);
            let p = std::path::Path::new(&abs);
            if p.exists() && p.is_dir() {
                if is_ancestor(&cwd, p) {
                    if seen_dirs.insert(p.to_path_buf()) {
                        tracing::debug!(
                            "Seeded parent_dirs with explicit directory: {}",
                            p.display()
                        );
                        parent_dirs.push(p.to_path_buf());
                    }
                } else {
                    tracing::debug!(
                        "Skipping explicit directory outside cwd boundary: {}",
                        p.display()
                    );
                }
            } else {
                tracing::debug!(
                    "Skipping explicit directory that does not exist or is not a dir: {}",
                    p.display()
                );
            }
        }
    }

    // Existing: seed from parent dirs of input files (skipping plan_structure.md)
    for f in files.iter() {
        if f.filename == "plan_structure.md" {
            continue;
        }
        let abs = to_abs_string(&f.filename);
        let p = std::path::Path::new(&abs);
        if let Some(parent) = p.parent()
            && is_ancestor(&cwd, parent)
            && seen_dirs.insert(parent.to_path_buf())
        {
            parent_dirs.push(parent.to_path_buf());
        }
    }

    if parent_dirs.is_empty() {
        tracing::debug!(
            "No explicit directories or file parents under cwd boundary; skipping CLAUDE.md injection"
        );
        return 0;
    }

    parent_dirs.sort_by_key(|d| d.components().count());

    let mut ordered_dirs = Vec::<std::path::PathBuf>::new();
    let mut seen_chain_dir = std::collections::HashSet::<std::path::PathBuf>::new();

    for parent in parent_dirs {
        if let Some(chain) = walk_up_to_boundary(&parent, &cwd) {
            for dir in chain {
                if seen_chain_dir.insert(dir.clone()) {
                    ordered_dirs.push(dir);
                }
            }
        }
    }

    let mut discovered = Vec::<std::path::PathBuf>::new();
    let mut seen_mem = std::collections::HashSet::<std::path::PathBuf>::new();

    for dir in ordered_dirs {
        for mf in memory_files_in_dir(&dir) {
            if seen_mem.insert(mf.clone()) {
                discovered.push(mf);
            }
        }
    }

    if discovered.is_empty() {
        tracing::debug!("No CLAUDE.md files found in directory chain");
        return 0;
    }

    let count_before = files.len();

    for mf in discovered {
        let dir = mf
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "<unknown>".to_string());

        tracing::debug!("Discovered memory file: {}", mf.display());

        files.push(FileMeta {
            filename: mf.to_string_lossy().to_string(),
            description: format!("Project memory from {}, auto-injected for context", dir),
        });
    }

    let injected = files.len() - count_before;
    tracing::info!("Auto-injected {} CLAUDE.md memory file(s)", injected);
    injected
}

#[cfg(test)]
mod claude_injection_tests {
    use super::*;
    use crate::test_support::{DirGuard, EnvGuard};
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_is_ancestor_basic() {
        let a = std::path::Path::new("/foo");
        let b = std::path::Path::new("/foo/bar");
        let c = std::path::Path::new("/bar");
        assert!(is_ancestor(a, b));
        assert!(is_ancestor(a, a));
        assert!(!is_ancestor(b, a));
        assert!(!is_ancestor(c, b));
    }

    #[test]
    fn test_walk_up_to_boundary_success() {
        let start = std::path::Path::new("/foo/bar/baz");
        let stop = std::path::Path::new("/foo");
        let result = walk_up_to_boundary(start, stop).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], std::path::Path::new("/foo"));
        assert_eq!(result[1], std::path::Path::new("/foo/bar"));
        assert_eq!(result[2], std::path::Path::new("/foo/bar/baz"));
    }

    #[test]
    fn test_walk_up_to_boundary_reject_when_not_ancestor() {
        let start = std::path::Path::new("/bar/baz");
        let stop = std::path::Path::new("/foo");
        let result = walk_up_to_boundary(start, stop);
        assert!(result.is_none());
    }

    #[test]
    fn test_memory_files_in_dir_variants() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        fs::write(root.join("CLAUDE.md"), "root").unwrap();
        fs::create_dir(root.join(".claude")).unwrap();
        fs::write(root.join(".claude").join("CLAUDE.md"), "dot").unwrap();

        let discovered = memory_files_in_dir(root);
        assert_eq!(discovered.len(), 2);
        assert!(discovered.iter().any(|p| p.ends_with("CLAUDE.md")));
        assert!(discovered.iter().any(|p| p.ends_with(".claude/CLAUDE.md")));
    }

    #[test]
    #[serial(env)]
    fn test_auto_inject_order_and_dedup() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        let a_dir = root.join("a");
        let b_dir = a_dir.join("b");
        fs::create_dir_all(&b_dir).unwrap();

        fs::write(root.join("CLAUDE.md"), "root").unwrap();
        fs::write(a_dir.join("CLAUDE.md"), "a").unwrap();
        fs::write(b_dir.join("CLAUDE.md"), "b").unwrap();

        let file_in_b = b_dir.join("code.rs");
        fs::write(&file_in_b, "fn f() {}").unwrap();

        let _g = DirGuard::set(root);

        let mut files = vec![FileMeta {
            filename: file_in_b.to_string_lossy().to_string(),
            description: "code".into(),
        }];

        let count = auto_inject_claude_memories(&mut files, None);
        assert_eq!(count, 3);
        assert_eq!(files.len(), 4);

        let names: Vec<_> = files.iter().map(|f| &f.filename).collect();
        let root_idx = names
            .iter()
            .position(|n| n.ends_with("CLAUDE.md") && !n.contains("a/") && !n.contains("b/"))
            .unwrap();
        let a_idx = names
            .iter()
            .position(|n| n.ends_with("a/CLAUDE.md"))
            .unwrap();
        let b_idx = names
            .iter()
            .position(|n| n.ends_with("b/CLAUDE.md"))
            .unwrap();
        assert!(root_idx < a_idx && a_idx < b_idx);
    }

    #[test]
    #[serial(env)]
    fn test_auto_inject_skips_outside_cwd() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        fs::write(root.join("CLAUDE.md"), "root").unwrap();

        let outside = TempDir::new().unwrap();
        let file_outside = outside.path().join("external.rs");
        fs::write(&file_outside, "fn e() {}").unwrap();

        let _g = DirGuard::set(root);

        let mut files = vec![FileMeta {
            filename: file_outside.to_string_lossy().to_string(),
            description: "external".into(),
        }];

        let count = auto_inject_claude_memories(&mut files, None);
        assert_eq!(count, 0);
        assert_eq!(files.len(), 1);
    }

    #[test]
    #[serial(env)]
    fn test_env_gate_unset_is_true() {
        let _g = EnvGuard::remove("INJECT_CLAUDE_MD");
        assert!(injection_enabled_from_env());
    }

    #[test]
    #[serial(env)]
    fn test_env_gate_1_is_true() {
        let _g = EnvGuard::set("INJECT_CLAUDE_MD", "1");
        assert!(injection_enabled_from_env());
    }

    #[test]
    #[serial(env)]
    fn test_env_gate_true_is_true() {
        let _g = EnvGuard::set("INJECT_CLAUDE_MD", "true");
        assert!(injection_enabled_from_env());
    }

    #[test]
    #[serial(env)]
    fn test_env_gate_0_is_false() {
        let _g = EnvGuard::set("INJECT_CLAUDE_MD", "0");
        assert!(!injection_enabled_from_env());
    }

    #[test]
    #[serial(env)]
    fn test_env_gate_false_is_false() {
        let _g = EnvGuard::set("INJECT_CLAUDE_MD", "false");
        assert!(!injection_enabled_from_env());
    }
}

#[cfg(test)]
mod claude_injection_integration_tests {
    use super::*;
    use crate::template::inject_files;
    use crate::test_support::{DirGuard, EnvGuard};
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    #[serial(env)]
    async fn test_e2e_template_injection_with_discovered_claude() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("CLAUDE.md"), "# Project Guide\nDo X").unwrap();
        fs::write(src.join("code.rs"), "fn f() {}").unwrap();

        let _g = DirGuard::set(root);

        let mut files = vec![FileMeta {
            filename: src.join("code.rs").to_string_lossy().to_string(),
            description: "impl".into(),
        }];

        let count = auto_inject_claude_memories(&mut files, None);
        assert_eq!(count, 1, "should inject root CLAUDE.md");

        let xml = r#"<context>
<!-- GROUP: implementation -->
</context>"#;

        use crate::optimizer::parser::{FileGroup, FileGrouping};
        let groups = FileGrouping {
            file_groups: vec![FileGroup {
                name: "implementation".to_string(),
                purpose: None,
                critical: None,
                files: files.iter().map(|f| f.filename.clone()).collect(),
            }],
        };

        let final_prompt = inject_files(xml, &groups).await.unwrap();
        assert!(final_prompt.contains("# Project Guide"));
        assert!(final_prompt.contains("Do X"));
    }

    #[test]
    #[serial(env)]
    fn test_env_gate_disables_injection() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        fs::write(root.join("CLAUDE.md"), "mem").unwrap();
        let file = root.join("code.rs");
        fs::write(&file, "fn f() {}").unwrap();

        let _g1 = DirGuard::set(root);
        let _g2 = EnvGuard::set("INJECT_CLAUDE_MD", "0");

        let mut files = vec![FileMeta {
            filename: file.to_string_lossy().to_string(),
            description: "code".into(),
        }];

        // Simulate pipeline behavior - check env gate before calling
        let count = if injection_enabled_from_env() {
            auto_inject_claude_memories(&mut files, None)
        } else {
            0
        };

        assert_eq!(
            count, 0,
            "injection should be disabled when INJECT_CLAUDE_MD=0"
        );
        assert_eq!(files.len(), 1);
    }
}

#[cfg(test)]
mod claude_injection_edge_tests {
    use super::*;
    use crate::test_support::DirGuard;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    #[serial(env)]
    fn test_multiple_subtrees() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        fs::write(root.join("CLAUDE.md"), "root").unwrap();
        fs::write(a.join("CLAUDE.md"), "a").unwrap();
        fs::write(b.join("CLAUDE.md"), "b").unwrap();

        let file_a = a.join("x.rs");
        let file_b = b.join("y.rs");
        fs::write(&file_a, "fn x() {}").unwrap();
        fs::write(&file_b, "fn y() {}").unwrap();

        let _g = DirGuard::set(root);

        let mut files = vec![
            FileMeta {
                filename: file_a.to_string_lossy().to_string(),
                description: "x".into(),
            },
            FileMeta {
                filename: file_b.to_string_lossy().to_string(),
                description: "y".into(),
            },
        ];

        let count = auto_inject_claude_memories(&mut files, None);
        assert_eq!(count, 3, "root, a, b memories");
        assert_eq!(files.len(), 5);
    }

    #[test]
    #[serial(env)]
    fn test_file_in_cwd_only() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        fs::write(root.join("CLAUDE.md"), "root").unwrap();
        let file = root.join("code.rs");
        fs::write(&file, "fn f() {}").unwrap();

        let _g = DirGuard::set(root);

        let mut files = vec![FileMeta {
            filename: file.to_string_lossy().to_string(),
            description: "code".into(),
        }];

        let count = auto_inject_claude_memories(&mut files, None);
        assert_eq!(count, 1);
        assert!(files[1].filename.ends_with("CLAUDE.md"));
    }

    #[test]
    #[serial(env)]
    fn test_no_input_files_no_injection() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        fs::write(root.join("CLAUDE.md"), "root").unwrap();

        let _g = DirGuard::set(root);

        let mut files = vec![];
        let count = auto_inject_claude_memories(&mut files, None);
        assert_eq!(count, 0);
    }

    #[test]
    #[serial(env)]
    fn test_dedup_when_user_passed_claude() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        let claude_path = root.join("CLAUDE.md");
        fs::write(&claude_path, "root").unwrap();

        let file = root.join("code.rs");
        fs::write(&file, "fn f() {}").unwrap();

        let _g = DirGuard::set(root);

        let mut files = vec![
            FileMeta {
                filename: claude_path.to_string_lossy().to_string(),
                description: "user-provided".into(),
            },
            FileMeta {
                filename: file.to_string_lossy().to_string(),
                description: "code".into(),
            },
        ];

        let count = auto_inject_claude_memories(&mut files, None);
        assert_eq!(count, 1);
        assert_eq!(files.len(), 3);
    }

    #[test]
    #[serial(env)]
    fn test_skips_plan_structure() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        fs::write(root.join("CLAUDE.md"), "root").unwrap();

        let _g = DirGuard::set(root);

        let mut files = vec![FileMeta {
            filename: "plan_structure.md".into(),
            description: "embedded".into(),
        }];

        let count = auto_inject_claude_memories(&mut files, None);
        assert_eq!(count, 0, "plan_structure.md has no real parent");
    }
}

#[cfg(test)]
mod claude_directory_seeding_tests {
    use super::*;
    use crate::test_support::DirGuard;
    use crate::types::DirectoryMeta;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    // 1) Regression: Zero-file directory should inject CLAUDE.md
    #[test]
    #[serial(env)]
    fn test_dir_seeding_injects_when_no_files_match() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("CLAUDE.md"), "docs").unwrap();
        fs::write(docs.join("README.md"), "readme").unwrap();

        let _g = DirGuard::set(root);

        let dirs = vec![DirectoryMeta {
            directory_path: docs.to_string_lossy().to_string(),
            description: "docs".into(),
            extensions: Some(vec!["rs".into()]), // expansion would yield zero if used
            recursive: false,
            include_hidden: false,
            max_files: crate::types::default_max_files(),
        }];

        let mut files: Vec<FileMeta> = vec![]; // simulate zero expanded files
        let count = auto_inject_claude_memories(&mut files, Some(&dirs));
        assert_eq!(count, 1, "should inject docs/CLAUDE.md despite zero files");
        assert_eq!(files.len(), 1);
        assert!(files[0].filename.ends_with("docs/CLAUDE.md"));
    }

    // 2) Ancestor Chain Test: explicit directory + ancestor CLAUDE.md
    #[test]
    #[serial(env)]
    fn test_explicit_dir_ancestor_chain_order() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(root.join("CLAUDE.md"), "root").unwrap();
        fs::write(docs.join("CLAUDE.md"), "docs").unwrap();

        let _g = DirGuard::set(root);

        let dirs = vec![DirectoryMeta {
            directory_path: docs.to_string_lossy().to_string(),
            description: "docs".into(),
            extensions: None,
            recursive: false,
            include_hidden: false,
            max_files: crate::types::default_max_files(),
        }];

        let mut files: Vec<FileMeta> = vec![];
        let count = auto_inject_claude_memories(&mut files, Some(&dirs));
        assert_eq!(count, 2, "root and docs CLAUDE.md should be injected");
        assert_eq!(files.len(), 2);
        let names: Vec<_> = files.iter().map(|f| f.filename.clone()).collect();
        let root_idx = names
            .iter()
            .position(|n| n.ends_with("/CLAUDE.md") && !n.contains("/docs/"))
            .unwrap();
        let docs_idx = names
            .iter()
            .position(|n| n.ends_with("/docs/CLAUDE.md"))
            .unwrap();
        assert!(root_idx < docs_idx, "root-to-leaf ordering must hold");
    }

    // 3) CWD Boundary Test: explicit directory outside CWD
    #[test]
    #[serial(env)]
    fn test_explicit_dir_outside_cwd_no_injection() {
        let td_a = TempDir::new().unwrap();
        let td_b = TempDir::new().unwrap();
        let root_a = td_a.path();
        let root_b = td_b.path();
        let docs_b = root_b.join("docs");
        std::fs::create_dir_all(&docs_b).unwrap();
        fs::write(docs_b.join("CLAUDE.md"), "bdocs").unwrap();

        let _g = DirGuard::set(root_a);

        let dirs = vec![DirectoryMeta {
            directory_path: docs_b.to_string_lossy().to_string(),
            description: "external docs".into(),
            extensions: None,
            recursive: false,
            include_hidden: false,
            max_files: crate::types::default_max_files(),
        }];

        let mut files: Vec<FileMeta> = vec![];
        let count = auto_inject_claude_memories(&mut files, Some(&dirs));
        assert_eq!(count, 0, "outside-cwd explicit directories must be ignored");
        assert_eq!(files.len(), 0);
    }

    // 4) Overlap Dedup: same directory as file parent and explicit
    #[test]
    #[serial(env)]
    fn test_overlap_explicit_and_file_parent_dedup() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(root.join("CLAUDE.md"), "root").unwrap();
        fs::write(docs.join("CLAUDE.md"), "docs").unwrap();
        fs::write(docs.join("code.rs"), "fn x() {}").unwrap();

        let _g = DirGuard::set(root);

        let dirs = vec![DirectoryMeta {
            directory_path: docs.to_string_lossy().to_string(),
            description: "docs".into(),
            extensions: None,
            recursive: false,
            include_hidden: false,
            max_files: crate::types::default_max_files(),
        }];

        let mut files = vec![FileMeta {
            filename: docs.join("code.rs").to_string_lossy().to_string(),
            description: "code".into(),
        }];

        let count = auto_inject_claude_memories(&mut files, Some(&dirs));
        assert_eq!(count, 2, "root and docs memories injected once each");
        let names: Vec<_> = files.iter().map(|f| f.filename.clone()).collect();
        assert_eq!(
            names
                .iter()
                .filter(|n| n.ends_with("/docs/CLAUDE.md"))
                .count(),
            1
        );
        assert_eq!(
            names
                .iter()
                .filter(|n| n.ends_with("/CLAUDE.md") && !n.contains("/docs/"))
                .count(),
            1
        );
    }

    // 5) Nested Directories Test: multiple explicit directories in same tree
    #[test]
    #[serial(env)]
    fn test_multiple_explicit_nested_dirs_injection_order() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        let sub = root.join("sub");
        let nested = sub.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("CLAUDE.md"), "root").unwrap();
        fs::write(sub.join("CLAUDE.md"), "sub").unwrap();
        fs::write(nested.join("CLAUDE.md"), "nested").unwrap();

        let _g = DirGuard::set(root);

        let dirs = vec![
            DirectoryMeta {
                directory_path: sub.to_string_lossy().to_string(),
                description: "sub".into(),
                extensions: None,
                recursive: false,
                include_hidden: false,
                max_files: crate::types::default_max_files(),
            },
            DirectoryMeta {
                directory_path: nested.to_string_lossy().to_string(),
                description: "nested".into(),
                extensions: None,
                recursive: false,
                include_hidden: false,
                max_files: crate::types::default_max_files(),
            },
        ];

        let mut files: Vec<FileMeta> = vec![];
        let count = auto_inject_claude_memories(&mut files, Some(&dirs));
        assert_eq!(count, 3, "root, sub, nested should be injected");
        let names: Vec<_> = files.iter().map(|f| f.filename.clone()).collect();
        let root_idx = names
            .iter()
            .position(|n| n.ends_with("/CLAUDE.md") && !n.contains("/sub/"))
            .unwrap();
        let sub_idx = names
            .iter()
            .position(|n| n.ends_with("/sub/CLAUDE.md"))
            .unwrap();
        let nested_idx = names
            .iter()
            .position(|n| n.ends_with("/sub/nested/CLAUDE.md"))
            .unwrap();
        assert!(root_idx < sub_idx && sub_idx < nested_idx);
    }

    // 6) Defensive: Nonexistent directory path
    #[test]
    #[serial(env)]
    fn test_nonexistent_explicit_directory_ignored() {
        let td = TempDir::new().unwrap();
        let root = td.path();

        let _g = DirGuard::set(root);

        let nonexistent = root.join("nope");
        let dirs = vec![DirectoryMeta {
            directory_path: nonexistent.to_string_lossy().to_string(),
            description: "nope".into(),
            extensions: None,
            recursive: false,
            include_hidden: false,
            max_files: crate::types::default_max_files(),
        }];

        let mut files: Vec<FileMeta> = vec![];
        let count = auto_inject_claude_memories(&mut files, Some(&dirs));
        assert_eq!(count, 0);
        assert!(files.is_empty());
    }
}
