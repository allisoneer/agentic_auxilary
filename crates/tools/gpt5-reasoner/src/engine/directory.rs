use crate::errors::{ReasonerError, Result};
use crate::types::{DirectoryMeta, FileMeta};
use std::collections::HashSet;
use walkdir::WalkDir;

fn ext_matches(filter: &Option<Vec<String>>, path: &std::path::Path) -> bool {
    match filter {
        None => true,
        Some(exts) if exts.is_empty() => true,
        Some(exts) => {
            let file_ext = match path.extension() {
                Some(e) => e.to_string_lossy().to_string(),
                None => return false,
            };
            let file_ext_norm = file_ext.trim_start_matches('.').to_ascii_lowercase();
            exts.iter().any(|e| {
                e.trim_start_matches('.')
                    .eq_ignore_ascii_case(&file_ext_norm)
            })
        }
    }
}

pub fn expand_directories_to_filemeta(directories: &[DirectoryMeta]) -> Result<Vec<FileMeta>> {
    let mut out = Vec::new();
    let mut seen = HashSet::<String>::new();
    for dir in directories {
        let walker = WalkDir::new(&dir.directory_path)
            .min_depth(1)
            .max_depth(if dir.recursive { usize::MAX } else { 1 })
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                if dir.include_hidden {
                    return true;
                }
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.')
            });

        let mut dir_file_count = 0;
        let mut skipped_binary = 0;
        let mut skipped_other = 0;

        for entry in walker {
            if dir_file_count >= dir.max_files {
                tracing::warn!(
                    "Directory '{}' hit max_files limit of {}; stopping traversal",
                    dir.directory_path,
                    dir.max_files
                );
                break;
            }

            let entry = entry.map_err(|e| {
                ReasonerError::Io(std::io::Error::other(format!(
                    "Walk error in {}: {}",
                    dir.directory_path, e
                )))
            })?;

            if !entry.file_type().is_file() {
                continue;
            }

            let file_name = entry.file_name().to_string_lossy();
            if !dir.include_hidden && file_name.starts_with('.') {
                continue;
            }

            let path = entry.path();
            if !ext_matches(&dir.extensions, path) {
                skipped_other += 1;
                continue;
            }

            match std::fs::read_to_string(path) {
                Ok(_) => {}
                Err(_) => {
                    skipped_binary += 1;
                    tracing::debug!("Skipping binary/non-UTF-8 file: {}", path.display());
                    continue;
                }
            }

            let path_str = if path.is_absolute() {
                path.to_string_lossy().to_string()
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(path))
                    .unwrap_or_else(|_| path.to_path_buf())
                    .to_string_lossy()
                    .to_string()
            };

            if seen.insert(path_str.clone()) {
                out.push(FileMeta {
                    filename: path_str,
                    description: dir.description.clone(),
                });
                dir_file_count += 1;
            }
        }

        tracing::debug!(
            "Expanded directory '{}': {} files (skipped {} binary, {} filtered)",
            dir.directory_path,
            dir_file_count,
            skipped_binary,
            skipped_other
        );
    }

    tracing::info!("Total files from directories: {}", out.len());
    Ok(out)
}

#[cfg(test)]
mod directory_expansion_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(p: &std::path::Path, content: &str) {
        fs::write(p, content).unwrap();
    }

    #[test]
    fn test_expand_non_recursive_ext_filter_and_hidden() {
        let td = TempDir::new().unwrap();
        let root = td.path();

        let f_rs = root.join("a.rs");
        let f_txt = root.join("b.txt");
        let f_hidden = root.join(".hidden.rs");
        write(&f_rs, "fn a() {}");
        write(&f_txt, "hello");
        write(&f_hidden, "hidden");

        let sub = root.join("sub");
        fs::create_dir_all(&sub).unwrap();
        let sub_rs = sub.join("c.rs");
        write(&sub_rs, "fn c() {}");

        let dirs = vec![DirectoryMeta {
            directory_path: root.to_string_lossy().to_string(),
            description: "test".into(),
            extensions: Some(vec!["rs".into(), ".RS".into()]),
            recursive: false,
            include_hidden: false,
            max_files: 1000,
        }];

        let files = expand_directories_to_filemeta(&dirs).unwrap();
        let names: Vec<_> = files.iter().map(|f| f.filename.clone()).collect();

        assert!(names.iter().any(|p| p.ends_with("a.rs")));
        assert!(!names.iter().any(|p| p.ends_with("b.txt")));
        assert!(!names.iter().any(|p| p.ends_with(".hidden.rs")));
        assert!(!names.iter().any(|p| p.ends_with("c.rs")));
    }

    #[test]
    fn test_expand_recursive_include_hidden_and_no_filter() {
        let td = TempDir::new().unwrap();
        let root = td.path();

        let f1 = root.join(".hidden.md");
        let f2 = root.join("readme.MD");
        let sub = root.join("src");
        fs::create_dir_all(&sub).unwrap();
        let f3 = sub.join("lib.Rs");
        write(&f1, "h");
        write(&f2, "r");
        write(&f3, "l");

        let dirs = vec![DirectoryMeta {
            directory_path: root.to_string_lossy().to_string(),
            description: "all".into(),
            extensions: None,
            recursive: true,
            include_hidden: true,
            max_files: 1000,
        }];

        let files = expand_directories_to_filemeta(&dirs).unwrap();
        let names: Vec<_> = files.iter().map(|f| f.filename.clone()).collect();
        assert!(names.iter().any(|p| p.ends_with(".hidden.md")));
        assert!(names.iter().any(|p| p.ends_with("readme.MD")));
        assert!(names.iter().any(|p| p.ends_with("lib.Rs")));
    }

    #[test]
    fn test_expand_dedup_across_directories() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        let f = root.join("x.rs");
        fs::write(&f, "//").unwrap();

        let dirs = vec![
            DirectoryMeta {
                directory_path: root.to_string_lossy().to_string(),
                description: "d1".into(),
                extensions: Some(vec!["rs".into()]),
                recursive: false,
                include_hidden: false,
                max_files: 1000,
            },
            DirectoryMeta {
                directory_path: root.to_string_lossy().to_string(),
                description: "d2".into(),
                extensions: Some(vec![".rs".into()]),
                recursive: false,
                include_hidden: false,
                max_files: 1000,
            },
        ];

        let files = expand_directories_to_filemeta(&dirs).unwrap();
        assert_eq!(files.len(), 1, "should dedup same path across entries");
    }

    #[test]
    fn test_hidden_directory_pruned() {
        let td = TempDir::new().unwrap();
        let root = td.path();

        let hidden_dir = root.join(".hidden");
        fs::create_dir_all(&hidden_dir).unwrap();
        let hidden_file = hidden_dir.join("secret.rs");
        write(&hidden_file, "fn secret() {}");

        let visible = root.join("visible.rs");
        write(&visible, "fn visible() {}");

        let dirs = vec![DirectoryMeta {
            directory_path: root.to_string_lossy().to_string(),
            description: "test".into(),
            extensions: Some(vec!["rs".into()]),
            recursive: true,
            include_hidden: false,
            max_files: 1000,
        }];

        let files = expand_directories_to_filemeta(&dirs).unwrap();
        let names: Vec<_> = files.iter().map(|f| f.filename.clone()).collect();

        assert!(names.iter().any(|p| p.ends_with("visible.rs")));
        assert!(
            !names.iter().any(|p| p.contains(".hidden")),
            "hidden directory should be pruned"
        );
    }

    #[test]
    fn test_nonexistent_directory() {
        let dirs = vec![DirectoryMeta {
            directory_path: "/nonexistent/path/12345".into(),
            description: "test".into(),
            extensions: None,
            recursive: false,
            include_hidden: false,
            max_files: 1000,
        }];

        let result = expand_directories_to_filemeta(&dirs);
        assert!(result.is_err(), "should error on nonexistent directory");
    }

    #[test]
    fn test_empty_extensions_vec_is_no_filter() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        write(&root.join("a.rs"), "rs");
        write(&root.join("b.txt"), "txt");

        let dirs = vec![DirectoryMeta {
            directory_path: root.to_string_lossy().to_string(),
            description: "test".into(),
            extensions: Some(vec![]),
            recursive: false,
            include_hidden: false,
            max_files: 1000,
        }];

        let files = expand_directories_to_filemeta(&dirs).unwrap();
        assert_eq!(
            files.len(),
            2,
            "empty extensions vec should include all files"
        );
    }

    #[test]
    fn test_max_files_cap() {
        let td = TempDir::new().unwrap();
        let root = td.path();

        for i in 0..10 {
            write(&root.join(format!("file{}.txt", i)), "content");
        }

        let dirs = vec![DirectoryMeta {
            directory_path: root.to_string_lossy().to_string(),
            description: "test".into(),
            extensions: None,
            recursive: false,
            include_hidden: false,
            max_files: 5,
        }];

        let files = expand_directories_to_filemeta(&dirs).unwrap();
        assert_eq!(files.len(), 5, "should stop at max_files cap");
    }
}
