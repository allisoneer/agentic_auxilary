use agentic_tools_core::ToolError;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

const RELATIVE_PATH_GUIDANCE: &str = "Use workspace-relative paths such as `src/main.rs`. Absolute paths are only allowed when they resolve inside the current workspace root.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    pub absolute_path: PathBuf,
    pub display_path: String,
}

pub fn resolve_workspace_path(root: &Path, input: &str) -> Result<ResolvedPath, ToolError> {
    if input.trim().is_empty() {
        return Err(ToolError::InvalidInput(format!(
            "Path is required. {RELATIVE_PATH_GUIDANCE}"
        )));
    }

    let canonical_root = std::fs::canonicalize(root).map_err(|error| {
        ToolError::Internal(format!(
            "Failed to resolve workspace root {}: {error}",
            root.display()
        ))
    })?;

    let requested = PathBuf::from(input);
    let combined = if requested.is_absolute() {
        requested
    } else {
        canonical_root.join(requested)
    };
    let absolute_path = canonicalize_existing_or_ancestor(&combined)?;

    if !absolute_path.starts_with(&canonical_root) {
        return Err(ToolError::InvalidInput(format!(
            "Path `{input}` resolves outside the workspace root. {RELATIVE_PATH_GUIDANCE}"
        )));
    }

    Ok(ResolvedPath {
        display_path: workspace_relative_display(&canonical_root, &absolute_path)?,
        absolute_path,
    })
}

fn canonicalize_existing_or_ancestor(path: &Path) -> Result<PathBuf, ToolError> {
    if path.exists() {
        return std::fs::canonicalize(path).map_err(|error| {
            ToolError::Internal(format!(
                "Failed to canonicalize {}: {error}",
                path.display()
            ))
        });
    }

    let mut ancestor = path;
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or_else(|| {
            ToolError::InvalidInput(format!(
                "Path `{}` is invalid. {RELATIVE_PATH_GUIDANCE}",
                path.display()
            ))
        })?;
    }

    let canonical_ancestor = std::fs::canonicalize(ancestor).map_err(|error| {
        ToolError::Internal(format!(
            "Failed to canonicalize {}: {error}",
            ancestor.display()
        ))
    })?;
    let suffix = path.strip_prefix(ancestor).map_err(|error| {
        ToolError::Internal(format!("Failed to resolve {}: {error}", path.display()))
    })?;

    normalize_joined_path(&canonical_ancestor, suffix)
}

fn normalize_joined_path(base: &Path, suffix: &Path) -> Result<PathBuf, ToolError> {
    let mut normalized = PathBuf::from(base);

    for component in suffix.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(ToolError::InvalidInput(format!(
                        "Path `{}` escapes the workspace root. {RELATIVE_PATH_GUIDANCE}",
                        suffix.display()
                    )));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ToolError::InvalidInput(
                    "Use a relative path such as `src/main.rs`. Absolute paths are only allowed when they resolve inside the workspace root.".to_string(),
                ));
            }
        }
    }

    Ok(normalized)
}

fn workspace_relative_display(root: &Path, path: &Path) -> Result<String, ToolError> {
    path.strip_prefix(root)
        .map_err(|error| ToolError::Internal(error.to_string()))
        .map(|relative| {
            let value = relative.to_string_lossy().replace('\\', "/");
            if value.is_empty() {
                String::from(".")
            } else {
                value
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            path.push(format!("{prefix}{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn relative_path_inside_root_succeeds() {
        let dir = TestDir::new("workspace-paths-");
        std::fs::create_dir_all(dir.path.join("src")).unwrap();
        std::fs::write(dir.path.join("src/main.rs"), "fn main() {}\n").unwrap();

        let resolved = resolve_workspace_path(&dir.path, "src/main.rs").unwrap();

        assert_eq!(resolved.display_path, "src/main.rs");
        assert!(resolved.absolute_path.ends_with("src/main.rs"));
    }

    #[test]
    fn absolute_path_inside_root_succeeds() {
        let dir = TestDir::new("workspace-paths-");
        std::fs::write(dir.path.join("file.txt"), "hello\n").unwrap();
        let input = dir.path.join("file.txt");

        let resolved = resolve_workspace_path(&dir.path, &input.to_string_lossy()).unwrap();

        assert_eq!(resolved.display_path, "file.txt");
    }

    #[test]
    fn dot_dot_traversal_fails() {
        let dir = TestDir::new("workspace-paths-");

        let error = resolve_workspace_path(&dir.path, "../outside.txt").unwrap_err();

        assert!(error.to_string().contains("outside the workspace root"));
    }

    #[test]
    fn absolute_path_outside_root_fails() {
        let dir = TestDir::new("workspace-paths-");
        let outside = TestDir::new("workspace-paths-outside-");

        let error = resolve_workspace_path(&dir.path, &outside.path.to_string_lossy()).unwrap_err();

        assert!(error.to_string().contains("outside the workspace root"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_fails() {
        use std::os::unix::fs::symlink;

        let dir = TestDir::new("workspace-paths-");
        let outside = TestDir::new("workspace-paths-outside-");
        symlink(&outside.path, dir.path.join("link")).unwrap();

        let error = resolve_workspace_path(&dir.path, "link/escape.txt").unwrap_err();

        assert!(error.to_string().contains("outside the workspace root"));
    }
}
