pub mod client;
pub mod errors;
pub mod optimizer;
pub mod template;
pub mod token;

use crate::{
    client::OrClient,
    errors::*,
    optimizer::{call_optimizer, parser::parse_optimizer_output},
    template::inject_files,
    token::enforce_limit,
};
// Add these imports for guard manipulation
use crate::optimizer::parser::{FileGroup, OptimizerOutput};
use async_openai::types::*;
use serde::{Deserialize, Serialize};
use universal_tool_core::prelude::*;
use walkdir::WalkDir;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileMeta {
    pub filename: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum PromptType {
    Reasoning,
    Plan,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DirectoryMeta {
    pub directory_path: String,
    pub description: String,
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub include_hidden: bool,
    /// Maximum number of files to include from this directory (default: 1000)
    #[serde(default = "default_max_files")]
    pub max_files: usize,
}

fn default_max_files() -> usize {
    1000
}

#[derive(Clone, Default)]
pub struct Gpt5Reasoner;

#[universal_tool_router(
    cli(name = "gpt5_reasoner"), // we won't use generated CLI, but harmless
    mcp(name = "gpt5_reasoner", version = "0.1.0")
)]
impl Gpt5Reasoner {
    #[universal_tool(description = "Optimize a prompt using file metadata and execute with GPT-5")]
    pub async fn optimize_and_execute(
        &self,
        prompt: String,
        files: Vec<FileMeta>,
        #[universal_tool_param(description = "Directories to expand into files before optimization")]
        directories: Option<Vec<DirectoryMeta>>,
        prompt_type: PromptType,
    ) -> std::result::Result<String, ToolError> {
        gpt5_reasoner_impl(prompt, files, directories, None, prompt_type).await
    }
}

// Helper: extension normalization and check
fn ext_matches(filter: &Option<Vec<String>>, path: &std::path::Path) -> bool {
    match filter {
        None => true,
        Some(exts) if exts.is_empty() => true,
        Some(exts) => {
            let file_ext = match path.extension() {
                Some(e) => e.to_string_lossy().to_string(),
                None => return false, // skip files with no extension when a filter is provided
            };
            let file_ext_norm = file_ext.trim_start_matches('.').to_ascii_lowercase();
            exts.iter().any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(&file_ext_norm))
        }
    }
}

// Helper: expand directories to FileMeta
fn expand_directories_to_filemeta(directories: &[DirectoryMeta]) -> Result<Vec<FileMeta>> {
    let mut out = Vec::new();
    let mut seen = HashSet::<String>::new();

    for dir in directories {
        // Build walker with filter_entry to prune hidden directories when include_hidden=false
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
            // Check max_files cap for this directory
            if dir_file_count >= dir.max_files {
                tracing::warn!(
                    "Directory '{}' hit max_files limit of {}; stopping traversal",
                    dir.directory_path,
                    dir.max_files
                );
                break;
            }

            let entry = entry.map_err(|e| {
                ReasonerError::Io(std::io::Error::other(
                    format!("Walk error in {}: {}", dir.directory_path, e)
                ))
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

            // Skip binary files - attempt to read as UTF-8
            match std::fs::read_to_string(path) {
                Ok(_) => {
                    // File is valid UTF-8, continue processing
                }
                Err(_) => {
                    // File is binary or not UTF-8 - skip it
                    skipped_binary += 1;
                    tracing::debug!("Skipping binary/non-UTF-8 file: {}", path.display());
                    continue;
                }
            }

            // Normalize path to absolute without resolving symlinks
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

// Helper: select optimizer model with precedence: param > env > default
fn select_optimizer_model(optimizer_model: Option<String>) -> String {
    optimizer_model
        .or_else(|| std::env::var("OPTIMIZER_MODEL").ok())
        .unwrap_or_else(|| "anthropic/claude-sonnet-4.5".to_string())
}

pub async fn gpt5_reasoner_impl(
    prompt: String,
    mut files: Vec<FileMeta>,
    directories: Option<Vec<DirectoryMeta>>,
    optimizer_model: Option<String>,
    prompt_type: PromptType,
) -> std::result::Result<String, ToolError> {
    // Expand directories to files BEFORE optimizer sees them
    if let Some(dirs) = directories.as_ref() {
        let mut expanded = expand_directories_to_filemeta(dirs)
            .map_err(ToolError::from)?;
        files.append(&mut expanded);

        // Dedup (by filename) to avoid duplicates from files + directories
        // Note: paths are normalized to absolute in expand_directories_to_filemeta
        let mut seen = HashSet::<String>::new();
        files.retain(|f| seen.insert(f.filename.clone()));
    }

    // Auto-inject plan_structure.md for Plan prompts (before optimizer)
    maybe_inject_plan_structure_meta(&prompt_type, &mut files);

    // Load env OpenRouter key (CLI already optionally did dotenv)
    let client = OrClient::from_env().map_err(ToolError::from)?;

    // Step 1: optimize
    // Default changed to anthropic/claude-sonnet-4.5, preserving param/env override order
    let opt_model = select_optimizer_model(optimizer_model);

    let raw = call_optimizer(&client, &opt_model, &prompt_type, &prompt, &files)
        .await
        .map_err(ToolError::from)?;

    // Debug: Print the raw optimizer output if RUST_LOG is set
    tracing::debug!("Raw optimizer output:\n{}", raw);

    let mut parsed = parse_optimizer_output(&raw).map_err(|e| {
        // On parse error, include the raw output for debugging
        tracing::error!("Failed to parse optimizer output:\n{}", raw);
        ToolError::from(e)
    })?;

    tracing::debug!(
        "Parsed optimizer output: {} groups found",
        parsed.groups.file_groups.len()
    );
    for group in &parsed.groups.file_groups {
        tracing::debug!("  Group '{}': {} files", group.name, group.files.len());
    }

    // Executor-side guard: ensure plan_template group and safe marker
    if matches!(prompt_type, PromptType::Plan) {
        ensure_plan_template_group(&mut parsed);
    }

    // Step 2: inject, token check, execute
    let mut final_prompt = inject_files(&parsed.xml_template, &parsed.groups)
        .await
        .map_err(ToolError::from)?;

    // Replace the {original_prompt} placeholder with the actual prompt
    final_prompt = final_prompt.replace("{original_prompt}", &prompt);

    let token_count = crate::token::count_tokens(&final_prompt).map_err(ToolError::from)?;
    tracing::debug!("Final prompt token count: {}", token_count);
    tracing::debug!(
        "Final prompt after injection (first 500 chars):\n{}...",
        final_prompt.chars().take(500).collect::<String>()
    );

    enforce_limit(&final_prompt).map_err(ToolError::from)?;

    // Execute GPT-5 with application-level retries for network/transport errors
    const GPT5_RETRIES: usize = 1;
    const GPT5_DELAY: std::time::Duration = std::time::Duration::from_millis(750);

    tracing::debug!("Executing final prompt with openai/gpt-5 at high reasoning effort");

    for attempt in 0..=GPT5_RETRIES {
        if attempt > 0 {
            tracing::warn!("GPT-5 API attempt {} of {}", attempt + 1, GPT5_RETRIES + 1);
            tokio::time::sleep(GPT5_DELAY).await;
        }

        // Build request inside the loop; clone final_prompt to keep ownership
        let req = CreateChatCompletionRequestArgs::default()
            .model("openai/gpt-5")
            .messages([ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(final_prompt.clone())
                    .build()
                    .map_err(|e| ToolError::from(ReasonerError::from(e)))?,
            )])
            .reasoning_effort(ReasoningEffort::High)
            .temperature(0.2)
            .build()
            .map_err(|e| ToolError::from(ReasonerError::from(e)))?;

        let start = std::time::Instant::now();
        match client.client.chat().create(req).await {
            Ok(resp) => {
                let duration = start.elapsed();
                tracing::debug!("GPT-5 API succeeded in {:?}", duration);

                let content = resp
                    .choices
                    .first()
                    .and_then(|c| c.message.content.clone())
                    .ok_or_else(|| {
                        ToolError::new(
                            universal_tool_core::error::ErrorCode::ExecutionFailed,
                            "GPT-5 returned empty content",
                        )
                    })?;

                return Ok(content);
            }
            Err(e) => {
                let retryable = crate::errors::is_retryable_app_level(&e);
                if attempt < GPT5_RETRIES && retryable {
                    tracing::warn!("GPT-5 call failed with retryable error: {e}; retrying...");
                    continue;
                }

                // Not retryable or retries exhausted
                if retryable {
                    tracing::error!(
                        "GPT-5 call failed after {} attempts with retryable error: {}",
                        attempt + 1,
                        e
                    );
                } else {
                    tracing::error!("GPT-5 call failed with non-retryable error: {}", e);
                }
                return Err(ToolError::from(ReasonerError::from(e)));
            }
        }
    }

    // Should never reach here due to loop logic, but provide a defensive error
    Err(ToolError::new(
        universal_tool_core::error::ErrorCode::ExecutionFailed,
        "GPT-5 failed after all retries",
    ))
}

// Helper: auto-inject plan_structure.md into files meta for Plan prompts
fn maybe_inject_plan_structure_meta(prompt_type: &PromptType, files: &mut Vec<FileMeta>) -> bool {
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
            // TODO(2): Allow aliasing and localization of plan template filename via config.
            return true;
        }
    }
    false
}

// Helper: ensure XML contains a safe-inserted marker for the given group
fn ensure_xml_has_group_marker(xml: &str, group_name: &str) -> String {
    let marker = format!("<!-- GROUP: {} -->", group_name);
    if xml.contains(&marker) {
        return xml.to_string();
    }

    // Strategy:
    // 1) If there are existing group markers, insert right after the last one.
    if let Some(pos) = xml.rfind("<!-- GROUP:") {
        // Insert after the end of that marker's line
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

    // 2) If there is a </context> closing tag, insert before it, honoring indentation if possible.
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

    // 3) Fallback: append at end on a new line.
    let mut out = xml.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&marker);
    out
}

// Helper: ensure plan_template group and its marker exist; ensure group references plan_structure.md
fn ensure_plan_template_group(parsed: &mut OptimizerOutput) {
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
        // Prepend for visibility; not strictly required for correctness
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

    // Ensure XML marker exists (safe insertion)
    parsed.xml_template = ensure_xml_has_group_marker(&parsed.xml_template, "plan_template");

    // TODO(2): Migrate to semi-frozen template approach with typed toggles, eliminating text marker concerns.
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
        // Should appear after the last group marker for 'b'
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

        // Group should exist and include plan_structure.md
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
        // Simulate optimizer output without plan_template
        let raw_yaml = r#"
```yaml
file_groups:
  - name: implementation_targets
    files: []
```

```xml
<context>
  <!-- GROUP: implementation_targets -->
</context>
```
"#;

        let mut parsed = parse_optimizer_output(raw_yaml).unwrap();

        // Apply guard
        ensure_plan_template_group(&mut parsed);

        // Inject
        let final_prompt = inject_files(&parsed.xml_template, &parsed.groups)
            .await
            .unwrap();

        // Verify plan structure is present
        assert!(final_prompt.contains("# [Feature/Task Name] Implementation Plan"));
        assert!(final_prompt.contains("## Overview"));
    }
}

#[cfg(test)]
mod model_selection_tests {
    use super::*;
    use serial_test::serial;

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, val: &str) -> Self {
            let prev = std::env::var(key).ok();
            // SAFETY: Tests are serialized via #[serial(env)], preventing concurrent access
            unsafe { std::env::set_var(key, val) };
            Self { key, prev }
        }
        fn remove(key: &'static str) -> Self {
            let prev = std::env::var(key).ok();
            // SAFETY: Tests are serialized via #[serial(env)], preventing concurrent access
            unsafe { std::env::remove_var(key) };
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: Tests are serialized via #[serial(env)], preventing concurrent access
            match &self.prev {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[test]
    #[serial(env)]
    fn test_default_model_when_no_param_no_env() {
        let _g = EnvGuard::remove("OPTIMIZER_MODEL");
        let model = select_optimizer_model(None);
        assert_eq!(model, "anthropic/claude-sonnet-4.5");
    }

    #[test]
    #[serial(env)]
    fn test_env_overrides_default() {
        let _g = EnvGuard::set("OPTIMIZER_MODEL", "test/model-from-env");
        let model = select_optimizer_model(None);
        assert_eq!(model, "test/model-from-env");
    }

    #[test]
    #[serial(env)]
    fn test_param_overrides_env_and_default() {
        let _g = EnvGuard::set("OPTIMIZER_MODEL", "test/env-model");
        let model = select_optimizer_model(Some("test/param-model".into()));
        assert_eq!(model, "test/param-model");
    }
}

#[cfg(test)]
mod directory_expansion_tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn write(p: &std::path::Path, content: &str) {
        fs::write(p, content).unwrap();
    }

    #[test]
    fn test_expand_non_recursive_ext_filter_and_hidden() {
        let td = TempDir::new().unwrap();
        let root = td.path();

        // Files at root
        let f_rs = root.join("a.rs");
        let f_txt = root.join("b.txt");
        let f_hidden = root.join(".hidden.rs");
        write(&f_rs, "fn a() {}");
        write(&f_txt, "hello");
        write(&f_hidden, "hidden");

        // Subdir with a file (should be skipped when non-recursive)
        let sub = root.join("sub");
        fs::create_dir_all(&sub).unwrap();
        let sub_rs = sub.join("c.rs");
        write(&sub_rs, "fn c() {}");

        let dirs = vec![DirectoryMeta {
            directory_path: root.to_string_lossy().to_string(),
            description: "test".into(),
            extensions: Some(vec!["rs".into(), ".RS".into()]), // case-insensitive, dot/no-dot
            recursive: false,
            include_hidden: false,
            max_files: 1000,
        }];

        let files = expand_directories_to_filemeta(&dirs).unwrap();
        let names: Vec<_> = files.iter().map(|f| f.filename.clone()).collect();

        assert!(names.iter().any(|p| p.ends_with("a.rs")));
        assert!(!names.iter().any(|p| p.ends_with("b.txt")));      // filtered by ext
        assert!(!names.iter().any(|p| p.ends_with(".hidden.rs"))); // hidden excluded
        assert!(!names.iter().any(|p| p.ends_with("c.rs")));       // non-recursive
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
            extensions: None, // no filter
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

        // Hidden directory with file inside
        let hidden_dir = root.join(".hidden");
        fs::create_dir_all(&hidden_dir).unwrap();
        let hidden_file = hidden_dir.join("secret.rs");
        write(&hidden_file, "fn secret() {}");

        // Visible file at root
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
        assert!(!names.iter().any(|p| p.contains(".hidden")), "hidden directory should be pruned");
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
            extensions: Some(vec![]), // empty = no filter
            recursive: false,
            include_hidden: false,
            max_files: 1000,
        }];

        let files = expand_directories_to_filemeta(&dirs).unwrap();
        assert_eq!(files.len(), 2, "empty extensions vec should include all files");
    }

    #[test]
    fn test_max_files_cap() {
        let td = TempDir::new().unwrap();
        let root = td.path();

        // Create 10 files
        for i in 0..10 {
            write(&root.join(format!("file{}.txt", i)), "content");
        }

        let dirs = vec![DirectoryMeta {
            directory_path: root.to_string_lossy().to_string(),
            description: "test".into(),
            extensions: None,
            recursive: false,
            include_hidden: false,
            max_files: 5, // Cap at 5
        }];

        let files = expand_directories_to_filemeta(&dirs).unwrap();
        assert_eq!(files.len(), 5, "should stop at max_files cap");
    }
}
