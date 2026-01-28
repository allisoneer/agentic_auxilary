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

/// Scanning strategy for collecting closing fence candidates
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum CandidateScanMode {
    /// Collect all closing fences greedily without stopping on inner language-tagged openers.
    /// Use for XML where inner content may have multiple language-tagged blocks.
    GreedyCollectAll,
    /// Stop collecting when encountering a language-tagged opener after finding at least one closer.
    /// Use for YAML to prevent spanning multiple consecutive blocks.
    StopOnLangOpenerAfterFirstCloser,
}

/// Collect all potential closing fence positions for validation.
///
/// Returns positions where valid closing fences could be (lines with ≥N backticks and only whitespace after).
/// Allows up to 3 leading spaces for markdown compatibility.
///
/// Mode behavior:
/// - GreedyCollectAll: Never stops on inner openers, collects all closers up to search_end
/// - StopOnLangOpenerAfterFirstCloser: Stops when hitting a language-tagged opener after finding a closer
fn collect_closing_candidates(
    s: &str,
    start: usize,
    fence_len: usize,
    search_end: usize,
    mode: CandidateScanMode,
) -> Vec<usize> {
    let mut candidates = Vec::new();
    let mut pos = start;
    let mut seen_closer = false;

    for line in s[start..search_end].split_inclusive('\n') {
        let line_start = pos;
        pos += line.len();

        // Allow up to 3 leading spaces (markdown spec)
        let trimmed_leading = line.trim_start_matches(' ');
        let leading_spaces = line.len() - trimmed_leading.len();
        if leading_spaces > 3 {
            continue;
        }

        let trimmed = trimmed_leading.trim_end();
        if !trimmed.starts_with('`') {
            continue;
        }

        let tick_count = trimmed.chars().take_while(|&c| c == '`').count();
        if tick_count < 3 {
            continue;
        }

        let after_ticks = &trimmed[tick_count..];
        let has_lang_tag = !after_ticks.trim().is_empty();

        if has_lang_tag {
            // This is an opening fence
            // In conservative mode, stop collecting if we've already found a closer
            if mode == CandidateScanMode::StopOnLangOpenerAfterFirstCloser && seen_closer {
                break;
            }
            // In greedy mode, continue collecting - don't stop on inner openers
        } else if tick_count >= fence_len {
            // This is a potential closing fence
            candidates.push(line_start);
            seen_closer = true;
        }
    }

    candidates
}

/// Extract a fenced code block anchored to a specific label, handling nested fences correctly.
///
/// Strategy (based on GPT-5 recommendation):
/// 1. Find the label and opening fence with N backticks
/// 2. Collect ALL closing fence candidates (≥N backticks, only whitespace after)
/// 3. Return the last candidate as a heuristic
///
/// For XML blocks, the caller (parse_optimizer_output) will validate candidates contain
/// required GROUP markers and pick the first valid one.
fn extract_fenced_block_fence_aware(s: &str, label: &str, lang: &str) -> Option<String> {
    // 1. Find label anchor
    let anchor_pos = s.find(label)?;
    let tail = &s[anchor_pos..];

    // 2. Scan lines for opening fence
    let mut char_offset = anchor_pos;
    let mut open_fence_end: Option<usize> = None;
    let mut fence_len = 0;

    for line in tail.split_inclusive('\n') {
        char_offset += line.len();

        let trimmed = line.trim_start();
        if !trimmed.starts_with('`') {
            continue;
        }

        // Count consecutive backticks
        let tick_count = trimmed.chars().take_while(|&c| c == '`').count();
        if tick_count < 3 {
            continue;
        }

        // Check for language tag after backticks
        let after_ticks = trimmed[tick_count..].trim_start();
        let first_word = after_ticks.split_whitespace().next().unwrap_or("");

        if first_word.eq_ignore_ascii_case(lang) {
            // Found opening fence
            open_fence_end = Some(char_offset); // Content starts after this line
            fence_len = tick_count;
            break;
        }
    }

    let content_start = open_fence_end?;

    // 3. Determine search boundary (stop at next label if present)
    let next_label_pos = s[content_start..]
        .find("FILE_GROUPING")
        .or_else(|| s[content_start..].find("OPTIMIZED_TEMPLATE"))
        .map(|offset| content_start + offset);

    let search_end = next_label_pos.unwrap_or(s.len());

    // 4. Collect closing fence candidates conservatively (stop on opener after closer)
    let candidates = collect_closing_candidates(
        s,
        content_start,
        fence_len,
        search_end,
        CandidateScanMode::StopOnLangOpenerAfterFirstCloser,
    );

    if candidates.is_empty() {
        return None;
    }

    // 5. Return FIRST candidate (the closer for the first block, not spanning multiple)
    let close_start = *candidates.first()?;

    // 6. Extract content between fences
    let content = &s[content_start..close_start];
    Some(content.to_string())
}

/// Count fence occurrences for diagnostic logging
fn log_fence_stats(raw: &str) {
    let triple_xml = raw.matches("```xml").count();
    let quad_xml = raw.matches("````xml").count();
    let triple_yaml = raw.matches("```yaml").count();
    let quad_yaml = raw.matches("````yaml").count();

    // Bare triple fences (without language tag) that might be closers or inner fences
    let bare_triple = raw
        .lines()
        .filter(|line| {
            let t = line.trim();
            t.starts_with("```")
                && t.chars().take_while(|&c| c == '`').count() == 3
                && t[3..].trim().is_empty()
        })
        .count();

    tracing::debug!(
        "Fence stats: yaml(3:{}, 4+:{}), xml(3:{}, 4+:{}), bare_triple_closers:{}",
        triple_yaml,
        quad_yaml,
        triple_xml,
        quad_xml,
        bare_triple
    );

    // Warn if multiple fences detected - indicates potential nesting
    if triple_xml + quad_xml > 1 || triple_yaml + quad_yaml > 1 {
        tracing::warn!(
            "Multiple fenced blocks detected; nested fences may be present (yaml: {}, xml: {})",
            triple_yaml + quad_yaml,
            triple_xml + quad_xml
        );
    }
}

/// Extract XML with validation: try each closing fence candidate until we find one
/// that contains all required GROUP markers.
fn extract_xml_with_validation(s: &str, label: &str, groups: &FileGrouping) -> Option<String> {
    // 1. Find label anchor and opening fence
    let anchor_pos = s.find(label)?;
    let tail = &s[anchor_pos..];

    let mut char_offset = anchor_pos;
    let mut open_fence_end: Option<usize> = None;
    let mut fence_len = 0;

    for line in tail.split_inclusive('\n') {
        char_offset += line.len();

        let trimmed = line.trim_start();
        if !trimmed.starts_with('`') {
            continue;
        }

        let tick_count = trimmed.chars().take_while(|&c| c == '`').count();
        if tick_count < 3 {
            continue;
        }

        let after_ticks = trimmed[tick_count..].trim_start();
        let first_word = after_ticks.split_whitespace().next().unwrap_or("");

        if first_word.eq_ignore_ascii_case("xml") {
            open_fence_end = Some(char_offset);
            fence_len = tick_count;
            break;
        }
    }

    let content_start = open_fence_end?;

    // 2. Determine search boundary
    let next_label_pos = s[content_start..]
        .find("FILE_GROUPING")
        .or_else(|| s[content_start..].find("OPTIMIZED_TEMPLATE"))
        .map(|offset| content_start + offset);

    let search_end = next_label_pos.unwrap_or(s.len());

    // 3. Collect all candidates greedily (don't stop on inner language-tagged openers)
    let candidates = collect_closing_candidates(
        s,
        content_start,
        fence_len,
        search_end,
        CandidateScanMode::GreedyCollectAll,
    );

    if candidates.is_empty() {
        return None;
    }

    tracing::debug!("Found {} XML closing fence candidates", candidates.len());

    // 4. Validate each candidate in order - pick first that has all GROUP markers
    for (idx, &close_start) in candidates.iter().enumerate() {
        let xml_candidate = &s[content_start..close_start];

        // Check if all GROUP markers are present
        let all_markers_present = groups.file_groups.iter().all(|g| {
            let marker = format!("<!-- GROUP: {} -->", g.name);
            xml_candidate.contains(&marker)
        });

        if all_markers_present {
            tracing::debug!(
                "Candidate {} (of {}) passed validation (length: {} chars)",
                idx + 1,
                candidates.len(),
                xml_candidate.len()
            );
            return Some(xml_candidate.to_string());
        } else {
            tracing::debug!(
                "Candidate {} (of {}) failed validation (length: {} chars)",
                idx + 1,
                candidates.len(),
                xml_candidate.len()
            );
        }
    }

    // 5. Fallback: use last candidate even if it doesn't validate
    // (will fail later with helpful error message)
    tracing::warn!("No candidates passed validation; using last candidate as fallback");
    let close_start = *candidates.last()?;
    Some(s[content_start..close_start].to_string())
}

pub fn parse_optimizer_output(raw: &str) -> Result<OptimizerOutput> {
    // Log fence statistics for debugging
    log_fence_stats(raw);

    // Clean up common formatting issues
    let cleaned = raw
        .replace("**FILE_GROUPING**", "FILE_GROUPING")
        .replace("**OPTIMIZED_TEMPLATE**", "OPTIMIZED_TEMPLATE");

    // Extract YAML with fence-aware extraction
    let yaml = extract_fenced_block_fence_aware(&cleaned, "FILE_GROUPING", "yaml")
        .or_else(|| extract_yaml_by_anchor(&cleaned))
        .ok_or_else(|| ReasonerError::Template("Could not find YAML FILE_GROUPING".into()))?;

    let groups: FileGrouping = serde_yaml::from_str(&yaml)?;

    // Extract XML with validation - tries each candidate until one passes
    let xml = extract_xml_with_validation(&cleaned, "OPTIMIZED_TEMPLATE", &groups)
        .ok_or_else(|| ReasonerError::Template("Could not find XML OPTIMIZED_TEMPLATE".into()))?;

    // Validate GROUP markers (should pass now, but keep for safety)
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

fn extract_yaml_by_anchor(s: &str) -> Option<String> {
    // simple heuristic: capture from "file_groups:" until next fenced block, label, or end
    let re = Regex::new(r"(?s)(file_groups:\s.*?)(```|FILE_GROUPING|OPTIMIZED_TEMPLATE|$)").ok()?;
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

FILE_GROUPING
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

OPTIMIZED_TEMPLATE
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
        let raw = r#"FILE_GROUPING
```yaml
file_groups:
  - name: missing_marker
    files:
      - src/lib.rs
```

OPTIMIZED_TEMPLATE
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
        let raw = r#"FILE_GROUPING
```yaml
file_groups:
  - purpose: Missing name field
    files:
      - src/lib.rs
```

OPTIMIZED_TEMPLATE
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

FILE_GROUPING
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
OPTIMIZED_TEMPLATE
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

FILE_GROUPING
file_groups:
  - name: fallback_test
    files:
      - test.rs

OPTIMIZED_TEMPLATE
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
        let raw = r#"FILE_GROUPING
```yaml
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
    fn test_extract_fenced_block_fence_aware_simple_case() {
        let content = "LABEL\n```rust\nfn main() {}\n```";
        let result = extract_fenced_block_fence_aware(content, "LABEL", "rust");
        assert_eq!(result, Some("fn main() {}\n".to_string()));

        let no_match = extract_fenced_block_fence_aware(content, "LABEL", "python");
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

    #[test]
    fn test_nested_triple_fences_inside_triple_xml() {
        let raw = r#"
FILE_GROUPING
```yaml
file_groups:
  - name: test
    files: [a.rs]
```

OPTIMIZED_TEMPLATE
```xml
<primary_task>
User prompt with code example:

```
fn main() {
    println!("hello");
}
```

More content after example...
</primary_task>

<codebase>
  <!-- GROUP: test -->
</codebase>
```
"#;

        let result = parse_optimizer_output(raw);
        assert!(result.is_ok(), "Parser should handle nested triple fences");

        let parsed = result.unwrap();
        assert!(parsed.xml_template.contains("<!-- GROUP: test -->"));
        assert!(
            parsed.xml_template.contains("fn main()"),
            "Should include content after nested fence"
        );
    }

    #[test]
    fn test_outer_four_backticks_with_inner_triple() {
        let raw = r#"
FILE_GROUPING
````yaml
file_groups:
  - name: demo
    files: [x.rs]
````

OPTIMIZED_TEMPLATE
````xml
<note>
Inner triple fences are safe:
```
code example
```
No collision!
</note>
<!-- GROUP: demo -->
````
"#;

        let result = parse_optimizer_output(raw);
        assert!(
            result.is_ok(),
            "Parser should support 4+ backtick outer fences"
        );

        let parsed = result.unwrap();
        assert!(parsed.xml_template.contains("<!-- GROUP: demo -->"));
        assert!(parsed.xml_template.contains("No collision!"));
    }

    #[test]
    fn test_label_anchoring_ignores_earlier_fences() {
        let raw = r#"
Here's an example of how to format the output:

```yaml
# This is NOT the FILE_GROUPING
example: value
```

Now here's the actual output:

FILE_GROUPING
```yaml
file_groups:
  - name: real
    files: [test.rs]
```

OPTIMIZED_TEMPLATE
```xml
<!-- GROUP: real -->
```
"#;

        let result = parse_optimizer_output(raw);
        assert!(
            result.is_ok(),
            "Label anchoring should ignore earlier example blocks"
        );

        let parsed = result.unwrap();
        assert_eq!(parsed.groups.file_groups[0].name, "real");
        assert!(!parsed.xml_template.contains("example: value"));
    }

    #[test]
    fn test_xml_mixed_bare_and_lang_tagged_inner_blocks() {
        // This test ensures we don't break early when XML contains multiple language-tagged blocks
        let raw = r#"
FILE_GROUPING
```yaml
file_groups:
  - name: mix
    files: [a.rs]
```

OPTIMIZED_TEMPLATE
```xml
<primary_task>
Example with bare fenced code:

```
bare fenced code
```

And a Python example:

```python
print("hi")
```

And YAML config:

```yaml
key: value
```

All before the marker!
</primary_task>
<!-- GROUP: mix -->
```
"#;

        let result = parse_optimizer_output(raw);
        assert!(
            result.is_ok(),
            "Parser should not stop at inner language-tagged openers (python, yaml)"
        );
        let parsed = result.unwrap();
        assert!(parsed.xml_template.contains("<!-- GROUP: mix -->"));
        assert!(parsed.xml_template.contains("bare fenced code"));
        assert!(parsed.xml_template.contains("print(\"hi\")"));
        assert!(parsed.xml_template.contains("key: value"));
        assert!(parsed.xml_template.contains("All before the marker!"));
    }
}
