pub struct InstallAsset {
    pub rel_path: &'static str,
    pub contents: &'static str,
}

macro_rules! include_repo_str {
    ($path:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../", $path))
    };
}

// Order matters: .opencode assets first, opencode.json last.
pub const INSTALL_MANIFEST: &[InstallAsset] = &[
    InstallAsset {
        rel_path: ".opencode/sysprompt.md",
        contents: include_repo_str!(".opencode/sysprompt.md"),
    },
    InstallAsset {
        rel_path: ".opencode/sysprompt_gpt54.md",
        contents: include_repo_str!(".opencode/sysprompt_gpt54.md"),
    },
    InstallAsset {
        rel_path: ".opencode/orchestrator_sysprompt_gpt54.md",
        contents: include_repo_str!(".opencode/orchestrator_sysprompt_gpt54.md"),
    },
    InstallAsset {
        rel_path: ".opencode/review_sysprompt_gpt54.md",
        contents: include_repo_str!(".opencode/review_sysprompt_gpt54.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/bash.md",
        contents: include_repo_str!(".opencode/command/bash.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/capture_pr_comments_openai.md",
        contents: include_repo_str!(".opencode/command/capture_pr_comments_openai.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/commit.md",
        contents: include_repo_str!(".opencode/command/commit.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/create_plan_final.md",
        contents: include_repo_str!(".opencode/command/create_plan_final.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/create_plan_init.md",
        contents: include_repo_str!(".opencode/command/create_plan_init.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/decide_findings_openai.md",
        contents: include_repo_str!(".opencode/command/decide_findings_openai.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/describe_pr.md",
        contents: include_repo_str!(".opencode/command/describe_pr.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/discord.md",
        contents: include_repo_str!(".opencode/command/discord.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/frame_openai.md",
        contents: include_repo_str!(".opencode/command/frame_openai.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/implement_plan.md",
        contents: include_repo_str!(".opencode/command/implement_plan.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/linear.md",
        contents: include_repo_str!(".opencode/command/linear.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/linear_ticket_2_pr.md",
        contents: include_repo_str!(".opencode/command/linear_ticket_2_pr.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/linear_ticket_design_brief.md",
        contents: include_repo_str!(".opencode/command/linear_ticket_design_brief.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/openai.md",
        contents: include_repo_str!(".opencode/command/openai.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/playwright.md",
        contents: include_repo_str!(".opencode/command/playwright.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/research.md",
        contents: include_repo_str!(".opencode/command/research.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/resolve_pr_comments.md",
        contents: include_repo_str!(".opencode/command/resolve_pr_comments.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/resume_work_openai.md",
        contents: include_repo_str!(".opencode/command/resume_work_openai.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/review.md",
        contents: include_repo_str!(".opencode/command/review.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/review_pr_comments.md",
        contents: include_repo_str!(".opencode/command/review_pr_comments.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/sync_with_main_and_resolve_conflicts.md",
        contents: include_repo_str!(".opencode/command/sync_with_main_and_resolve_conflicts.md"),
    },
    InstallAsset {
        rel_path: ".opencode/command/unwind_openai.md",
        contents: include_repo_str!(".opencode/command/unwind_openai.md"),
    },
    InstallAsset {
        rel_path: "opencode.json",
        contents: include_repo_str!("opencode.json"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::HashSet;

    #[test]
    fn manifest_ends_with_opencode_json() {
        assert_eq!(
            INSTALL_MANIFEST.last().map(|asset| asset.rel_path),
            Some("opencode.json")
        );
    }

    #[test]
    fn manifest_paths_are_relative_and_do_not_escape() {
        for asset in INSTALL_MANIFEST {
            let path = std::path::Path::new(asset.rel_path);
            assert!(
                !path.is_absolute(),
                "manifest path is absolute: {}",
                asset.rel_path
            );
            assert!(
                !path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir)),
                "manifest path escapes repo root: {}",
                asset.rel_path
            );
        }
    }

    #[test]
    fn opencode_json_file_refs_are_in_manifest() {
        let opencode = INSTALL_MANIFEST
            .iter()
            .find(|asset| asset.rel_path == "opencode.json")
            .expect("manifest should include opencode.json");

        let parsed: Value =
            serde_json::from_str(opencode.contents).expect("opencode.json should be valid JSON");
        let mut file_refs = Vec::new();
        collect_file_refs(&parsed, &mut file_refs);

        let manifest_paths = INSTALL_MANIFEST
            .iter()
            .map(|asset| asset.rel_path)
            .collect::<HashSet<_>>();

        for file_ref in file_refs {
            let normalized = file_ref.strip_prefix("./").unwrap_or(&file_ref);
            assert!(
                manifest_paths.contains(normalized),
                "missing manifest entry for file ref: {file_ref} (normalized: {normalized})"
            );
        }
    }

    fn collect_file_refs(value: &Value, output: &mut Vec<String>) {
        match value {
            Value::String(string) => {
                if let Some(inner) = string
                    .strip_prefix("{file:")
                    .and_then(|value| value.strip_suffix('}'))
                {
                    output.push(inner.to_string());
                }
            }
            Value::Array(values) => values
                .iter()
                .for_each(|value| collect_file_refs(value, output)),
            Value::Object(object) => object
                .values()
                .for_each(|value| collect_file_refs(value, output)),
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
}
