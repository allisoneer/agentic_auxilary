use crate::errors::ReasonerError;
use crate::types::FileMeta;
use universal_tool_core::prelude::ToolError;

pub fn to_abs_string(p: &str) -> String {
    let path = std::path::Path::new(p);
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical.to_string_lossy().to_string();
    }
    if path.is_absolute() {
        path.to_string_lossy().to_string()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string()
    }
}

pub fn normalize_paths_in_place(files: &mut [FileMeta]) {
    for f in files {
        if f.filename == "plan_structure.md" {
            continue;
        }
        f.filename = to_abs_string(&f.filename);
    }
}

pub fn dedup_files_in_place(files: &mut Vec<FileMeta>) {
    let mut seen = std::collections::HashSet::<String>::new();
    files.retain(|f| seen.insert(f.filename.clone()));
}

pub fn precheck_files(files: &[FileMeta]) -> Result<(), ToolError> {
    for f in files {
        if f.filename == "plan_structure.md" {
            continue;
        }
        let pb = std::path::PathBuf::from(&f.filename);
        if !pb.exists() {
            return Err(ToolError::from(ReasonerError::MissingFile(pb)));
        }
        let bytes = std::fs::read(&pb).map_err(ReasonerError::from)?;
        if String::from_utf8(bytes).is_err() {
            return Err(ToolError::from(ReasonerError::NonUtf8(pb)));
        }
    }
    Ok(())
}

pub fn is_ancestor(ancestor: &std::path::Path, descendant: &std::path::Path) -> bool {
    let anc = ancestor.components().collect::<Vec<_>>();
    let des = descendant.components().collect::<Vec<_>>();
    anc.len() <= des.len() && anc == des[..anc.len()]
}

pub fn walk_up_to_boundary(
    start: &std::path::Path,
    stop: &std::path::Path,
) -> Option<Vec<std::path::PathBuf>> {
    if !is_ancestor(stop, start) {
        return None;
    }
    let mut cur = start.to_path_buf();
    let mut chain = Vec::new();
    chain.push(cur.clone());
    while cur != stop {
        cur = match cur.parent() {
            Some(p) => p.to_path_buf(),
            None => break,
        };
        chain.push(cur.clone());
    }
    chain.reverse();
    Some(chain)
}

#[cfg(test)]
mod pre_validation_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    #[serial_test::serial]
    fn test_to_abs_string_makes_relative_absolute() {
        let rel = "foo/bar/baz.txt";
        let abs = to_abs_string(rel);
        assert!(std::path::Path::new(&abs).is_absolute());
    }

    #[test]
    fn test_to_abs_string_preserves_absolute() {
        let abs_path = "/home/user/file.rs";
        let result = to_abs_string(abs_path);
        assert_eq!(result, abs_path);
    }

    #[test]
    #[serial_test::serial]
    fn test_normalize_paths_in_place_skips_embedded() {
        let mut files = vec![
            FileMeta {
                filename: "plan_structure.md".into(),
                description: "embedded".into(),
            },
            FileMeta {
                filename: "a.rs".into(),
                description: "code".into(),
            },
        ];
        normalize_paths_in_place(&mut files);
        assert_eq!(files[0].filename, "plan_structure.md");
        assert!(std::path::Path::new(&files[1].filename).is_absolute());
    }

    #[test]
    #[serial_test::serial]
    fn test_dedup_files_in_place_across_rel_abs() {
        let td = TempDir::new().unwrap();
        let file = td.path().join("dup.rs");
        fs::write(&file, "code").unwrap();

        let abs = file.to_string_lossy().to_string();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(td.path()).unwrap();

        let mut files = vec![
            FileMeta {
                filename: "dup.rs".into(),
                description: "rel".into(),
            },
            FileMeta {
                filename: abs.clone(),
                description: "abs".into(),
            },
        ];

        normalize_paths_in_place(&mut files);
        dedup_files_in_place(&mut files);

        std::env::set_current_dir(orig_dir).unwrap();

        assert_eq!(
            files.len(),
            1,
            "duplicates should be removed after normalization"
        );
        assert!(files[0].filename.ends_with("dup.rs"));
        assert!(std::path::Path::new(&files[0].filename).is_absolute());
    }

    #[test]
    fn test_precheck_files_missing_fails_fast() {
        let files = vec![FileMeta {
            filename: "/nonexistent/file.xyz".into(),
            description: "missing".into(),
        }];
        let err = precheck_files(&files).unwrap_err();
        assert!(err.to_string().contains("File not found"));
    }

    #[test]
    fn test_precheck_files_non_utf8_fails_fast() {
        let td = TempDir::new().unwrap();
        let p = td.path().join("bin.dat");
        fs::write(&p, &[0xFF, 0xFE, 0xFD]).unwrap();
        let files = vec![FileMeta {
            filename: p.to_string_lossy().to_string(),
            description: "bin".into(),
        }];
        let err = precheck_files(&files).unwrap_err();
        assert!(err.to_string().contains("Unsupported file encoding"));
    }

    #[test]
    fn test_precheck_files_utf8_ok() {
        let td = TempDir::new().unwrap();
        let p = td.path().join("ok.txt");
        fs::write(&p, "hello").unwrap();
        let files = vec![FileMeta {
            filename: p.to_string_lossy().to_string(),
            description: "ok".into(),
        }];
        precheck_files(&files).unwrap();
    }

    #[test]
    fn test_precheck_files_skips_plan_structure() {
        let files = vec![FileMeta {
            filename: "plan_structure.md".into(),
            description: "embedded template".into(),
        }];
        precheck_files(&files).unwrap();
    }

    #[test]
    fn test_precheck_files_empty_file() {
        let td = TempDir::new().unwrap();
        let p = td.path().join("empty.txt");
        fs::write(&p, "").unwrap();
        let files = vec![FileMeta {
            filename: p.to_string_lossy().to_string(),
            description: "empty".into(),
        }];
        precheck_files(&files).unwrap();
    }
}
