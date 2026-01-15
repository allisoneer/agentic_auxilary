use crate::optimizer::parser::{FileGroup, OptimizerOutput};
use crate::types::{FileMeta, PromptType};

pub fn maybe_inject_plan_structure_meta(
    prompt_type: &PromptType,
    files: &mut Vec<FileMeta>,
) -> bool {
    if matches!(prompt_type, PromptType::Plan) {
        let has_plan = files.iter().any(|f| f.filename == "plan_structure.md");
        if !has_plan {
            tracing::info!(
                "Auto-injecting plan_structure.md into files array for PromptType::Plan"
            );
            files.insert(
                0,
                FileMeta {
                    filename: "plan_structure.md".to_string(),
                    description: "Plan output structure template (auto-injected)".to_string(),
                },
            );
            return true;
        }
    }
    false
}

pub fn ensure_xml_has_group_marker(xml: &str, group_name: &str) -> String {
    let marker = format!("<!-- GROUP: {} -->", group_name);
    if xml.contains(&marker) {
        return xml.to_string();
    }
    if let Some(pos) = xml.rfind("<!-- GROUP:") {
        let insert_pos = xml[pos..]
            .find('\n')
            .map(|off| pos + off + 1)
            .unwrap_or(xml.len());
        let mut out = String::with_capacity(xml.len() + marker.len() + 2);
        out.push_str(&xml[..insert_pos]);
        out.push_str(&marker);
        out.push('\n');
        out.push_str(&xml[insert_pos..]);
        return out;
    }
    if let Some(close_pos) = xml.rfind("</context>") {
        let line_start = xml[..close_pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let indent: String = xml[line_start..close_pos]
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        let mut out = String::with_capacity(xml.len() + marker.len() + indent.len() + 2);
        out.push_str(&xml[..close_pos]);
        out.push_str(&indent);
        out.push_str(&marker);
        out.push('\n');
        out.push_str(&xml[close_pos..]);
        return out;
    }
    let mut out = xml.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&marker);
    out
}

pub fn ensure_plan_template_group(parsed: &mut OptimizerOutput) {
    let mut has_group = false;
    for g in &parsed.groups.file_groups {
        if g.name == "plan_template" {
            has_group = true;
            break;
        }
    }

    if !has_group {
        tracing::warn!("Optimizer output missing 'plan_template' group; executor will insert it.");
        let new_group = FileGroup {
            name: "plan_template".to_string(),
            purpose: Some("Canonical plan template (executor guard).".to_string()),
            critical: Some(true),
            files: vec!["plan_structure.md".to_string()],
        };
        parsed.groups.file_groups.insert(0, new_group);
    } else if let Some(g) = parsed
        .groups
        .file_groups
        .iter_mut()
        .find(|g| g.name == "plan_template")
        && !g.files.iter().any(|f| f == "plan_structure.md")
    {
        tracing::warn!("'plan_template' group missing plan_structure.md; executor will add it.");
        g.files.insert(0, "plan_structure.md".to_string());
    }

    parsed.xml_template = ensure_xml_has_group_marker(&parsed.xml_template, "plan_template");
}

#[cfg(test)]
mod plan_guards_tests {
    use super::*;
    use crate::optimizer::parser::{FileGrouping, OptimizerOutput};

    #[test]
    fn test_maybe_inject_plan_structure_meta() {
        let mut files = vec![];
        let changed = maybe_inject_plan_structure_meta(&PromptType::Plan, &mut files);
        assert!(changed);
        assert_eq!(files[0].filename, "plan_structure.md");

        let changed_again = maybe_inject_plan_structure_meta(&PromptType::Plan, &mut files);
        assert!(!changed_again);
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_ensure_xml_has_group_marker_after_last_group() {
        let xml = "<context>\n  <!-- GROUP: a -->\n  <!-- GROUP: b -->\n</context>\n";
        let out = ensure_xml_has_group_marker(xml, "plan_template");
        assert!(out.contains("<!-- GROUP: plan_template -->"));
        let idx_b = out.find("<!-- GROUP: b -->").unwrap();
        let idx_pt = out.find("<!-- GROUP: plan_template -->").unwrap();
        assert!(idx_pt > idx_b);
    }

    #[test]
    fn test_ensure_xml_has_group_marker_before_context_close() {
        let xml = "<context>\n  <!-- none -->\n</context>\n";
        let out = ensure_xml_has_group_marker(xml, "plan_template");
        let pos_close = out.find("</context>").unwrap();
        let pos_marker = out.find("<!-- GROUP: plan_template -->").unwrap();
        assert!(pos_marker < pos_close);
    }

    #[test]
    fn test_ensure_plan_template_group_and_marker() {
        let groups = FileGrouping {
            file_groups: vec![],
        };
        let xml = "<context>\n  <!-- GROUP: other -->\n</context>\n".to_string();
        let mut parsed = OptimizerOutput {
            groups,
            xml_template: xml,
        };

        ensure_plan_template_group(&mut parsed);

        let g = parsed
            .groups
            .file_groups
            .iter()
            .find(|g| g.name == "plan_template")
            .unwrap();
        assert!(g.files.iter().any(|f| f == "plan_structure.md"));
        assert!(
            parsed
                .xml_template
                .contains("<!-- GROUP: plan_template -->")
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::optimizer::parser::parse_optimizer_output;
    use crate::template::inject_files;

    #[tokio::test]
    async fn test_end_to_end_plan_template_injection() {
        let raw_yaml = r#"
FILE_GROUPING
```yaml
file_groups:
  - name: implementation_targets
    files: []
```

OPTIMIZED_TEMPLATE
```xml
<context>
  <!-- GROUP: implementation_targets -->
</context>
```
"#;

        let mut parsed = parse_optimizer_output(raw_yaml).unwrap();

        ensure_plan_template_group(&mut parsed);

        let final_prompt = inject_files(&parsed.xml_template, &parsed.groups)
            .await
            .unwrap();

        assert!(final_prompt.contains("# [Feature/Task Name] Implementation Plan"));
        assert!(final_prompt.contains("## Overview"));
    }
}
