use crate::errors::*;
use crate::optimizer::parser::{FileGroup, FileGrouping};
use futures::stream::{self, StreamExt};
use std::path::PathBuf;
use tokio::fs;

async fn read_file_utf8(path: &str) -> Result<String> {
    let pb = PathBuf::from(path);
    if !pb.exists() {
        return Err(ReasonerError::MissingFile(pb));
    }
    let bytes = fs::read(&pb).await?;
    let content = String::from_utf8(bytes).map_err(|_| ReasonerError::NonUtf8(pb))?;
    Ok(content)
}

fn build_group_injection(group: &FileGroup, file_contents: &[(String, String)]) -> String {
    let mut out = String::new();
    out.push_str(&format!("<group name=\"{}\">\n", group.name));
    for (path, content) in file_contents {
        out.push_str(&format!("  <file path=\"{}\">\n", path));
        out.push_str(content);
        out.push_str("\n  </file>\n");
    }
    out.push_str("</group>");
    out
}

pub async fn inject_files(xml_template: &str, groups: &FileGrouping) -> Result<String> {
    // Preload all file contents, dedup by path, excluding embedded files
    let all_paths: Vec<String> = groups
        .file_groups
        .iter()
        .flat_map(|g| g.files.iter().cloned())
        .filter(|p| p != "plan_structure.md") // Exclude embedded template
        .collect();

    let unique_paths: Vec<String> = {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        all_paths
            .into_iter()
            .filter(|p| seen.insert(p.clone()))
            .collect()
    };

    let file_map_vec: Vec<(String, String)> = stream::iter(unique_paths.into_iter())
        .map(|p| async move {
            let content = read_file_utf8(&p).await?;
            Ok::<_, ReasonerError>((p, content))
        })
        .buffer_unordered(32)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?;

    let file_map: std::collections::HashMap<&str, &str> = file_map_vec
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    // Replace markers
    let mut final_xml = xml_template.to_string();

    for g in &groups.file_groups {
        let marker = format!("<!-- GROUP: {} -->", g.name);

        let contents: Vec<(String, String)> = g
            .files
            .iter()
            .map(|p| {
                // Special handling for plan_structure.md - use embedded content
                if p == "plan_structure.md" {
                    tracing::debug!("Using embedded plan_structure.md template");
                    Ok((
                        p.clone(),
                        crate::optimizer::prompts::PLAN_STRUCTURE_TEMPLATE.to_string(),
                    ))
                } else {
                    let content = file_map
                        .get(p.as_str())
                        .ok_or_else(|| ReasonerError::MissingFile(PathBuf::from(p)))?
                        .to_string();
                    Ok((p.clone(), content))
                }
            })
            .collect::<Result<Vec<_>>>()?;

        let injection = build_group_injection(g, &contents);
        final_xml = final_xml.replace(&marker, &injection);
    }

    Ok(final_xml)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimizer::parser::{FileGroup, FileGrouping};
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_read_file_utf8_success() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.rs");
        fs::write(&test_file, "fn main() {}").await.unwrap();

        let result = read_file_utf8(test_file.to_str().unwrap()).await.unwrap();
        assert_eq!(result, "fn main() {}");
    }

    #[tokio::test]
    async fn test_read_file_utf8_missing() {
        let result = read_file_utf8("/nonexistent/path/file.rs").await;
        match result {
            Err(ReasonerError::MissingFile(path)) => {
                assert_eq!(path.to_str().unwrap(), "/nonexistent/path/file.rs");
            }
            _ => panic!("Expected MissingFile error"),
        }
    }

    #[tokio::test]
    async fn test_read_file_utf8_non_utf8() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("binary.dat");
        // Write invalid UTF-8 bytes
        fs::write(&test_file, &[0xFF, 0xFE, 0xFD]).await.unwrap();

        let result = read_file_utf8(test_file.to_str().unwrap()).await;
        match result {
            Err(ReasonerError::NonUtf8(path)) => {
                assert_eq!(path.to_str().unwrap(), test_file.to_str().unwrap());
            }
            _ => panic!("Expected NonUtf8 error"),
        }
    }

    #[test]
    fn test_build_group_injection() {
        let group = FileGroup {
            name: "core".to_string(),
            purpose: Some("Core logic".to_string()),
            critical: Some(true),
            files: vec!["src/lib.rs".to_string(), "src/main.rs".to_string()],
        };

        let file_contents = vec![
            ("src/lib.rs".to_string(), "pub fn hello() {}".to_string()),
            (
                "src/main.rs".to_string(),
                "fn main() { hello(); }".to_string(),
            ),
        ];

        let result = build_group_injection(&group, &file_contents);
        let expected = r#"<group name="core">
  <file path="src/lib.rs">
pub fn hello() {}
  </file>
  <file path="src/main.rs">
fn main() { hello(); }
  </file>
</group>"#;

        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_inject_files_basic() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        let file1 = temp_dir.path().join("file1.rs");
        let file2 = temp_dir.path().join("file2.rs");
        fs::write(&file1, "// File 1 content").await.unwrap();
        fs::write(&file2, "// File 2 content").await.unwrap();

        let groups = FileGrouping {
            file_groups: vec![
                FileGroup {
                    name: "group1".to_string(),
                    purpose: None,
                    critical: None,
                    files: vec![file1.to_str().unwrap().to_string()],
                },
                FileGroup {
                    name: "group2".to_string(),
                    purpose: None,
                    critical: None,
                    files: vec![file2.to_str().unwrap().to_string()],
                },
            ],
        };

        let xml_template = r#"<context>
  <!-- GROUP: group1 -->
  <!-- GROUP: group2 -->
</context>"#;

        let result = inject_files(xml_template, &groups).await.unwrap();

        assert!(result.contains(r#"<group name="group1">"#));
        assert!(result.contains(r#"<group name="group2">"#));
        assert!(result.contains("// File 1 content"));
        assert!(result.contains("// File 2 content"));
        assert!(!result.contains("<!-- GROUP:")); // All markers replaced
    }

    #[tokio::test]
    async fn test_inject_files_duplicate_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create a shared file
        let shared_file = temp_dir.path().join("shared.rs");
        fs::write(&shared_file, "// Shared content").await.unwrap();

        let groups = FileGrouping {
            file_groups: vec![
                FileGroup {
                    name: "group1".to_string(),
                    purpose: None,
                    critical: None,
                    files: vec![shared_file.to_str().unwrap().to_string()],
                },
                FileGroup {
                    name: "group2".to_string(),
                    purpose: None,
                    critical: None,
                    files: vec![shared_file.to_str().unwrap().to_string()],
                },
            ],
        };

        let xml_template = r#"<!-- GROUP: group1 -->
<!-- GROUP: group2 -->"#;

        let result = inject_files(xml_template, &groups).await.unwrap();

        // Both groups should have the same file content
        assert_eq!(result.matches("// Shared content").count(), 2);
    }

    #[tokio::test]
    async fn test_inject_files_missing_file() {
        let groups = FileGrouping {
            file_groups: vec![FileGroup {
                name: "group1".to_string(),
                purpose: None,
                critical: None,
                files: vec!["/nonexistent/file.rs".to_string()],
            }],
        };

        let xml_template = "<!-- GROUP: group1 -->";

        let result = inject_files(xml_template, &groups).await;
        match result {
            Err(ReasonerError::MissingFile(path)) => {
                assert_eq!(path.to_str().unwrap(), "/nonexistent/file.rs");
            }
            _ => panic!("Expected MissingFile error"),
        }
    }
}
