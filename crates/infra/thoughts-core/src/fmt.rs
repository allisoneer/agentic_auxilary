//! TextFormat implementations for thoughts_tool output types.
//!
//! These implementations produce identical output to the McpFormatter
//! implementations, preserving Unicode symbols (checkmarks, dashes) for human-readable output.

use agentic_tools_core::fmt::TextFormat;
use agentic_tools_core::fmt::TextOptions;

use crate::documents::ActiveDocuments;
use crate::documents::WriteDocumentOk;
use crate::mcp::AddReferenceOk;
use crate::mcp::ReferencesList;
use crate::mcp::RepoRefsList;
use crate::mcp::TemplateResponse;
use crate::utils::human_size;

impl TextFormat for WriteDocumentOk {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let mut out = format!(
            "\u{2713} Created {}\n  Size: {}",
            self.path,
            human_size(self.bytes_written)
        );
        if let Some(url) = &self.github_url {
            out.push_str(&format!("\n  URL (after sync): {}", url));
        }
        out
    }
}

impl TextFormat for ActiveDocuments {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        if self.files.is_empty() {
            return format!(
                "Active base: {}\nFiles (relative to base):\n<none>",
                self.base
            );
        }
        let mut out = format!("Active base: {}\nFiles (relative to base):", self.base);
        for f in &self.files {
            let rel = f
                .path
                .strip_prefix(&format!("{}/", self.base.trim_end_matches('/')))
                .unwrap_or(&f.path);
            let ts = match chrono::DateTime::parse_from_rfc3339(&f.modified) {
                Ok(dt) => dt
                    .with_timezone(&chrono::Utc)
                    .format("%Y-%m-%d %H:%M UTC")
                    .to_string(),
                Err(_) => f.modified.clone(),
            };
            out.push_str(&format!("\n{} @ {}", rel, ts));
        }
        out
    }
}

impl TextFormat for ReferencesList {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        if self.entries.is_empty() {
            return format!("References base: {}\n<none>", self.base);
        }
        let mut out = format!("References base: {}", self.base);
        for e in &self.entries {
            let rel = e
                .path
                .strip_prefix(&format!("{}/", self.base.trim_end_matches('/')))
                .unwrap_or(&e.path);
            match &e.description {
                Some(desc) if !desc.trim().is_empty() => {
                    out.push_str(&format!("\n{} \u{2014} {}", rel, desc));
                }
                _ => {
                    out.push_str(&format!("\n{}", rel));
                }
            }
        }
        out
    }
}

impl TextFormat for RepoRefsList {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        if self.entries.is_empty() {
            return format!("Remote refs for {}\n<none>", self.url);
        }

        let mut out = if self.truncated {
            format!(
                "Remote refs for {} (showing {} of {}):",
                self.url,
                self.entries.len(),
                self.total
            )
        } else {
            format!("Remote refs for {} ({}):", self.url, self.total)
        };

        for entry in &self.entries {
            out.push_str(&format!("\n{}", entry.name));
            if let Some(oid) = &entry.oid {
                out.push_str(&format!(" oid={oid}"));
            }
            if let Some(peeled) = &entry.peeled {
                out.push_str(&format!(" peeled={peeled}"));
            }
            if let Some(target) = &entry.target {
                out.push_str(&format!(" target={target}"));
            }
        }

        out
    }
}

impl TextFormat for AddReferenceOk {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let mut out = String::new();
        if self.already_existed {
            out.push_str("\u{2713} Reference already exists (idempotent)\n");
        } else {
            out.push_str("\u{2713} Added reference\n");
        }
        out.push_str(&format!(
            "  URL: {}\n  Org/Repo: {}/{}",
            self.url, self.org, self.repo
        ));
        if let Some(ref_name) = &self.ref_name {
            out.push_str(&format!("\n  Ref: {}", ref_name));
        }
        out.push_str(&format!(
            "\n  Mount: {}\n  Target: {}",
            self.mount_path, self.mount_target
        ));
        if let Some(mp) = &self.mapping_path {
            out.push_str(&format!("\n  Mapping: {}", mp));
        } else {
            out.push_str("\n  Mapping: <none>");
        }
        out.push_str(&format!(
            "\n  Config updated: {}\n  Cloned: {}\n  Mounted: {}",
            self.config_updated, self.cloned, self.mounted
        ));
        if !self.warnings.is_empty() {
            out.push_str("\nWarnings:");
            for w in &self.warnings {
                out.push_str(&format!("\n- {}", w));
            }
        }
        out
    }
}

impl TextFormat for TemplateResponse {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let ty = self.template_type.label();
        let content = self.template_type.content();
        let guidance = self.template_type.guidance();
        format!(
            "Here is the {} template:\n\n```markdown\n{}\n```\n\n{}",
            ty, content, guidance
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::DocumentInfo;
    use crate::git::remote_refs::RemoteRef;
    use crate::mcp::ReferenceItem;
    use crate::mcp::TemplateType;

    #[test]
    fn write_document_text_format() {
        let v = WriteDocumentOk {
            path: "./thoughts/x/research/test.md".into(),
            bytes_written: 2048,
            github_url: None,
        };
        let tf = v.fmt_text(&TextOptions::default());
        assert!(tf.contains("\u{2713} Created"));
        assert!(tf.contains("2.0 KB"));
        assert!(!tf.contains("URL")); // No URL when github_url is None
    }

    #[test]
    fn write_document_text_format_with_url() {
        let v = WriteDocumentOk {
            path: "./thoughts/x/research/test.md".into(),
            bytes_written: 2048,
            github_url: Some("https://github.com/org/repo/blob/x/research/test.md".into()),
        };
        let tf = v.fmt_text(&TextOptions::default());
        assert!(tf.contains("\u{2713} Created"));
        assert!(tf.contains("2.0 KB"));
        assert!(tf.contains("URL (after sync): https://github.com/org/repo"));
    }

    #[test]
    fn active_documents_empty_text_format() {
        let docs = ActiveDocuments {
            base: "./thoughts/branch".into(),
            files: vec![],
        };
        let tf = docs.fmt_text(&TextOptions::default());
        assert!(tf.contains("<none>"));
    }

    #[test]
    fn active_documents_with_files_text_format() {
        let docs = ActiveDocuments {
            base: "./thoughts/feature".into(),
            files: vec![DocumentInfo {
                path: "./thoughts/feature/research/test.md".into(),
                doc_type: "research".into(),
                size: 1024,
                modified: "2025-10-15T12:00:00Z".into(),
            }],
        };
        let tf = docs.fmt_text(&TextOptions::default());
        assert!(tf.contains("research/test.md"));
    }

    #[test]
    fn references_list_empty_text_format() {
        let refs = ReferencesList {
            base: "references".into(),
            entries: vec![],
        };
        let tf = refs.fmt_text(&TextOptions::default());
        assert!(tf.contains("<none>"));
    }

    #[test]
    fn references_list_with_descriptions_text_format() {
        let refs = ReferencesList {
            base: "references".into(),
            entries: vec![
                ReferenceItem {
                    path: "references/org/repo1".into(),
                    description: Some("First repo".into()),
                },
                ReferenceItem {
                    path: "references/org/repo2".into(),
                    description: None,
                },
            ],
        };
        let tf = refs.fmt_text(&TextOptions::default());
        assert!(tf.contains("org/repo1 \u{2014} First repo"));
        assert!(tf.contains("org/repo2"));
    }

    #[test]
    fn repo_refs_list_text_format() {
        let refs = RepoRefsList {
            url: "https://github.com/org/repo".into(),
            total: 2,
            truncated: false,
            entries: vec![
                RemoteRef {
                    name: "refs/heads/main".into(),
                    oid: Some("abc123".into()),
                    peeled: None,
                    target: None,
                },
                RemoteRef {
                    name: "HEAD".into(),
                    oid: Some("abc123".into()),
                    peeled: None,
                    target: Some("refs/heads/main".into()),
                },
            ],
        };
        let tf = refs.fmt_text(&TextOptions::default());
        assert!(tf.contains("Remote refs for https://github.com/org/repo"));
        assert!(tf.contains("refs/heads/main oid=abc123"));
        assert!(tf.contains("HEAD oid=abc123 target=refs/heads/main"));
    }

    #[test]
    fn add_reference_ok_text_format() {
        let ok = AddReferenceOk {
            url: "https://github.com/org/repo".into(),
            ref_name: Some("refs/heads/main".into()),
            org: "org".into(),
            repo: "repo".into(),
            mount_path: "references/org/repo".into(),
            mount_target: "/abs/.thoughts-data/references/org/repo".into(),
            mapping_path: Some("/home/user/.thoughts/clones/repo".into()),
            already_existed: false,
            config_updated: true,
            cloned: true,
            mounted: true,
            warnings: vec!["note".into()],
        };
        let tf = ok.fmt_text(&TextOptions::default());
        assert!(tf.contains("\u{2713} Added reference"));
        assert!(tf.contains("Ref: refs/heads/main"));
        assert!(tf.contains("Warnings:\n- note"));
    }

    #[test]
    fn add_reference_ok_already_existed_text_format() {
        let ok = AddReferenceOk {
            url: "https://github.com/org/repo".into(),
            ref_name: None,
            org: "org".into(),
            repo: "repo".into(),
            mount_path: "references/org/repo".into(),
            mount_target: "/abs/.thoughts-data/references/org/repo".into(),
            mapping_path: None,
            already_existed: true,
            config_updated: false,
            cloned: false,
            mounted: true,
            warnings: vec![],
        };
        let tf = ok.fmt_text(&TextOptions::default());
        assert!(tf.contains("\u{2713} Reference already exists (idempotent)"));
        assert!(tf.contains("Mapping: <none>"));
    }

    #[test]
    fn template_response_text_format() {
        let resp = TemplateResponse {
            template_type: TemplateType::Research,
        };
        let tf = resp.fmt_text(&TextOptions::default());
        assert!(tf.starts_with("Here is the research template:"));
        assert!(tf.contains("```markdown"));
    }
}
