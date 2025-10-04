use crate::errors::*;
use regex::Regex;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct FileGroup {
    pub name: String,
    pub purpose: Option<String>,
    #[serde(default)]
    pub critical: Option<bool>,
    pub files: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileGrouping {
    pub file_groups: Vec<FileGroup>,
}

#[derive(Debug)]
pub struct OptimizerOutput {
    pub groups: FileGrouping,
    pub xml_template: String,
}

pub fn parse_optimizer_output(raw: &str) -> Result<OptimizerOutput> {
    // Clean up common formatting issues
    let cleaned = raw
        .replace("**FILE_GROUPING**", "FILE_GROUPING")
        .replace("**OPTIMIZED_TEMPLATE**", "OPTIMIZED_TEMPLATE");

    // 1) Try to extract fenced yaml
    let yaml = extract_fenced_block(&cleaned, "yaml")
        .or_else(|| extract_yaml_by_anchor(&cleaned))
        .ok_or_else(|| ReasonerError::Template("Could not find YAML FILE_GROUPING".into()))?;

    let groups: FileGrouping = serde_yaml::from_str(&yaml)?;

    // 2) Extract fenced xml
    let xml = extract_fenced_block(&cleaned, "xml")
        .ok_or_else(|| ReasonerError::Template("Could not find XML OPTIMIZED_TEMPLATE".into()))?;

    // 3) Validate markers for each group
    for g in &groups.file_groups {
        let marker = format!("<!-- GROUP: {} -->", g.name);
        if !xml.contains(&marker) {
            // Try to help debug by showing what markers were found
            let found_markers: Vec<String> = xml
                .lines()
                .filter(|line| {
                    line.trim().starts_with("<!-- GROUP:") && line.trim().ends_with("-->")
                })
                .map(|s| s.to_string())
                .collect();

            return Err(ReasonerError::Template(format!(
                "Template missing marker for group '{}'. Found markers: {:?}",
                g.name, found_markers
            )));
        }
    }

    Ok(OptimizerOutput {
        groups,
        xml_template: xml,
    })
}

fn extract_fenced_block(s: &str, lang: &str) -> Option<String> {
    let re = Regex::new(&format!(r"(?s)```{}\s*(.*?)```", regex::escape(lang))).ok()?;
    re.captures(s)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

fn extract_yaml_by_anchor(s: &str) -> Option<String> {
    // simple heuristic: capture from "file_groups:" until next fenced block or end
    let re = Regex::new(r"(?s)(file_groups:\s.*?)(```|$)").ok()?;
    re.captures(s)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_output_with_fenced_blocks() {
        let raw = r#"Here's the optimized output:

```yaml
file_groups:
  - name: core_logic
    purpose: Main business logic
    critical: true
    files:
      - src/lib.rs
      - src/main.rs
  - name: tests
    purpose: Test files
    files:
      - tests/integration.rs
```

And the template:

```xml
<codebase>
  <context>
    <!-- GROUP: core_logic -->
    <!-- GROUP: tests -->
  </context>
</codebase>
```

That should work!"#;

        let result = parse_optimizer_output(raw).unwrap();
        assert_eq!(result.groups.file_groups.len(), 2);
        assert_eq!(result.groups.file_groups[0].name, "core_logic");
        assert_eq!(result.groups.file_groups[0].critical, Some(true));
        assert_eq!(result.groups.file_groups[1].name, "tests");
        assert!(result.xml_template.contains("<!-- GROUP: core_logic -->"));
        assert!(result.xml_template.contains("<!-- GROUP: tests -->"));
    }

    #[test]
    fn test_missing_group_marker_error() {
        let raw = r#"```yaml
file_groups:
  - name: missing_marker
    files:
      - src/lib.rs
```

```xml
<codebase>
  <!-- No marker for missing_marker group -->
</codebase>
```"#;

        let err = parse_optimizer_output(raw).unwrap_err();
        match err {
            ReasonerError::Template(msg) => {
                assert!(msg.contains("Template missing marker for group 'missing_marker'"));
            }
            _ => panic!("Expected Template error"),
        }
    }

    #[test]
    fn test_yaml_missing_fields() {
        let raw = r#"```yaml
file_groups:
  - purpose: Missing name field
    files:
      - src/lib.rs
```

```xml
<codebase>
  <!-- GROUP: something -->
</codebase>
```"#;

        let err = parse_optimizer_output(raw).unwrap_err();
        match err {
            ReasonerError::Yaml(_) => {
                // Expected serde_yaml error for missing required field
            }
            _ => panic!("Expected Yaml error"),
        }
    }

    #[test]
    fn test_multiple_code_blocks_first_wins() {
        let raw = r#"First attempt:
```yaml
file_groups:
  - name: first
    files: [a.rs]
```

Second attempt (should be ignored):
```yaml
file_groups:
  - name: second
    files: [b.rs]
```

Template:
```xml
<!-- GROUP: first -->
```"#;

        let result = parse_optimizer_output(raw).unwrap();
        assert_eq!(result.groups.file_groups.len(), 1);
        assert_eq!(result.groups.file_groups[0].name, "first");
    }

    #[test]
    fn test_fallback_yaml_extraction() {
        let raw = r#"Here's the output without fenced blocks:

file_groups:
  - name: fallback_test
    files:
      - test.rs

```xml
<codebase>
  <!-- GROUP: fallback_test -->
</codebase>
```"#;

        let result = parse_optimizer_output(raw).unwrap();
        assert_eq!(result.groups.file_groups[0].name, "fallback_test");
    }

    #[test]
    fn test_no_yaml_found_error() {
        let raw = r#"Some text without any YAML

```xml
<codebase>
  <!-- Some XML -->
</codebase>
```"#;

        let err = parse_optimizer_output(raw).unwrap_err();
        match err {
            ReasonerError::Template(msg) => {
                assert!(msg.contains("Could not find YAML FILE_GROUPING"));
            }
            _ => panic!("Expected Template error"),
        }
    }

    #[test]
    fn test_no_xml_found_error() {
        let raw = r#"```yaml
file_groups:
  - name: test
    files: [test.rs]
```

But no XML template!"#;

        let err = parse_optimizer_output(raw).unwrap_err();
        match err {
            ReasonerError::Template(msg) => {
                assert!(msg.contains("Could not find XML OPTIMIZED_TEMPLATE"));
            }
            _ => panic!("Expected Template error"),
        }
    }

    #[test]
    fn test_extract_fenced_block() {
        let content = "```rust\nfn main() {}\n```";
        let result = extract_fenced_block(content, "rust");
        assert_eq!(result, Some("fn main() {}\n".to_string()));

        let no_match = extract_fenced_block(content, "python");
        assert!(no_match.is_none());
    }

    #[test]
    fn test_extract_yaml_by_anchor() {
        let content = r#"Some text
file_groups:
  - name: test
    files: [a.rs]
```
End"#;

        let result = extract_yaml_by_anchor(content);
        assert!(result.is_some());
        assert!(result.unwrap().contains("file_groups:"));
    }

    #[test]
    fn test_original_prompt_placeholder() {
        let raw = r#"FILE_GROUPING
```yaml
file_groups:
  - name: test
    files: []
```

OPTIMIZED_TEMPLATE
```xml
<primary_task>{original_prompt}</primary_task>
<!-- GROUP: test -->
```"#;

        let result = parse_optimizer_output(raw).unwrap();
        assert!(result.xml_template.contains("{original_prompt}"));
    }

    #[test]
    fn test_markdown_header_cleanup() {
        let raw = r#"**FILE_GROUPING**
```yaml
file_groups:
  - name: test
    files: []
```

**OPTIMIZED_TEMPLATE**
```xml
<!-- GROUP: test -->
```"#;

        // Should parse successfully despite markdown formatting
        let result = parse_optimizer_output(raw).unwrap();
        assert_eq!(result.groups.file_groups.len(), 1);
    }
}
