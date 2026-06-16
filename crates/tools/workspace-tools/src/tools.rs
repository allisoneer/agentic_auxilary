use agentic_config::types::WorkspaceToolsConfig;
use agentic_tools_core::BoxFuture;
use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use agentic_tools_core::ToolRegistry;
use atomicwrites::AtomicFile;
use atomicwrites::OverwriteBehavior;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use crate::patch::PatchChunk;
use crate::patch::PatchOp;
use crate::patch::apply_chunks;
use crate::patch::parse_patch;
use crate::paths::resolve_workspace_path;
use crate::service::TodoItem;
use crate::service::WorkspaceRuntime;

const DEFAULT_READ_LIMIT: usize = 2000;
const MAX_LINE_LENGTH: usize = 2000;
const TRUNCATION_SUFFIX: &str = "... (line truncated to 2000 chars)";
const BINARY_SAMPLE_BYTES: usize = 4096;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkspaceReadInput {
    #[schemars(description = "Use workspace-relative paths such as `src/main.rs`.")]
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkspaceTodoWriteInput {
    pub todos: Vec<TodoItem>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkspaceEditInput {
    #[schemars(description = "Use workspace-relative paths such as `src/main.rs`.")]
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "oldString")]
    pub old_string: String,
    #[serde(rename = "newString")]
    pub new_string: String,
    #[serde(default, rename = "replaceAll")]
    pub replace_all: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkspaceApplyPatchInput {
    #[serde(rename = "patchText")]
    pub patch_text: String,
}

#[derive(Clone)]
pub struct WorkspaceReadTool {
    pub runtime: Arc<WorkspaceRuntime>,
}

#[derive(Clone)]
pub struct WorkspaceTodoWriteTool {
    pub runtime: Arc<WorkspaceRuntime>,
}

#[derive(Clone)]
pub struct WorkspaceEditTool {
    pub runtime: Arc<WorkspaceRuntime>,
}

#[derive(Clone)]
pub struct WorkspaceApplyPatchTool {
    pub runtime: Arc<WorkspaceRuntime>,
}

impl Tool for WorkspaceReadTool {
    type Input = WorkspaceReadInput;
    type Output = String;
    const NAME: &'static str = "workspace_read";
    const DESCRIPTION: &'static str = "Read files or directories inside the current workspace. Prefer workspace-relative paths such as `src/main.rs`.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let runtime = Arc::clone(&self.runtime);
        Box::pin(async move { workspace_read(runtime.as_ref(), &input) })
    }
}

impl Tool for WorkspaceTodoWriteTool {
    type Input = WorkspaceTodoWriteInput;
    type Output = String;
    const NAME: &'static str = "workspace_todowrite";
    const DESCRIPTION: &'static str = "Replace the in-memory todo list for this MCP process. State resets when the server restarts.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let runtime = Arc::clone(&self.runtime);
        Box::pin(async move { workspace_todowrite(runtime.as_ref(), input) })
    }
}

impl Tool for WorkspaceEditTool {
    type Input = WorkspaceEditInput;
    type Output = String;
    const NAME: &'static str = "workspace_edit";
    const DESCRIPTION: &'static str = "Edit a file inside the current workspace using exact string replacement. Prefer workspace-relative paths.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let runtime = Arc::clone(&self.runtime);
        Box::pin(async move { workspace_edit(runtime.as_ref(), input).await })
    }
}

impl Tool for WorkspaceApplyPatchTool {
    type Input = WorkspaceApplyPatchInput;
    type Output = String;
    const NAME: &'static str = "workspace_apply_patch";
    const DESCRIPTION: &'static str = "Apply an OpenCode-style patch inside the current workspace. Prefer workspace-relative paths.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let runtime = Arc::clone(&self.runtime);
        Box::pin(async move { workspace_apply_patch(runtime.as_ref(), &input) })
    }
}

pub fn build_registry(config: &WorkspaceToolsConfig) -> ToolRegistry {
    let runtime = Arc::new(WorkspaceRuntime::discover());
    let mut builder = ToolRegistry::builder();

    if config.workspace_read {
        builder = builder.register::<WorkspaceReadTool, ()>(WorkspaceReadTool {
            runtime: Arc::clone(&runtime),
        });
    }
    if config.workspace_todowrite {
        builder = builder.register::<WorkspaceTodoWriteTool, ()>(WorkspaceTodoWriteTool {
            runtime: Arc::clone(&runtime),
        });
    }
    if config.workspace_edit {
        builder = builder.register::<WorkspaceEditTool, ()>(WorkspaceEditTool {
            runtime: Arc::clone(&runtime),
        });
    }
    if config.workspace_apply_patch {
        builder =
            builder.register::<WorkspaceApplyPatchTool, ()>(WorkspaceApplyPatchTool { runtime });
    }

    builder.finish()
}

fn workspace_read(
    runtime: &WorkspaceRuntime,
    input: &WorkspaceReadInput,
) -> Result<String, ToolError> {
    let tools = runtime.tools()?;
    let resolved = resolve_workspace_path(tools.root(), &input.file_path)?;
    let metadata = fs::metadata(&resolved.absolute_path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => ToolError::NotFound(format!(
            "Path `{}` was not found. Use workspace-relative paths such as `src/main.rs`.",
            resolved.display_path
        )),
        _ => ToolError::Internal(format!(
            "Failed to inspect {}: {error}",
            resolved.display_path
        )),
    })?;

    if metadata.is_dir() {
        return render_directory(
            &resolved.absolute_path,
            &resolved.display_path,
            input.offset,
            input.limit,
        );
    }

    render_file(
        &resolved.absolute_path,
        &resolved.display_path,
        input.offset,
        input.limit,
    )
}

fn workspace_todowrite(
    runtime: &WorkspaceRuntime,
    input: WorkspaceTodoWriteInput,
) -> Result<String, ToolError> {
    let tools = runtime.tools()?;
    let todos = tools.replace_todos(input.todos)?;
    serde_json::to_string_pretty(&todos).map_err(|error| ToolError::Internal(error.to_string()))
}

async fn workspace_edit(
    runtime: &WorkspaceRuntime,
    input: WorkspaceEditInput,
) -> Result<String, ToolError> {
    if input.old_string == input.new_string {
        return Err(ToolError::InvalidInput(
            "oldString and newString must be different.".into(),
        ));
    }

    let tools = runtime.tools()?;
    let resolved = resolve_workspace_path(tools.root(), &input.file_path)?;
    let file_lock = tools.file_lock(&resolved.absolute_path)?;
    let _guard = file_lock.lock().await;

    if input.old_string.is_empty() {
        let existing = read_text_file_if_exists(&resolved.absolute_path, &resolved.display_path)?;
        let (new_bom, new_text) = split_bom(&input.new_string);
        let desired_bom = existing
            .as_ref()
            .map_or(new_bom, |file| file.bom || new_bom);
        write_text_file(&resolved.absolute_path, &new_text, desired_bom)?;
        let verb = if existing.is_some() {
            "Updated"
        } else {
            "Created"
        };
        return Ok(format!("{verb} {}", resolved.display_path));
    }

    let source = read_text_file(&resolved.absolute_path, &resolved.display_path)?;
    let old = normalize_to_line_ending(&input.old_string, source.line_ending);
    let (new_bom, new_text_raw) = split_bom(&input.new_string);
    let replacement = normalize_to_line_ending(&new_text_raw, source.line_ending);
    let matches = source.text.matches(&old).count();

    if matches == 0 {
        return Err(ToolError::InvalidInput(format!(
            "oldString did not match any text in {}.",
            resolved.display_path
        )));
    }

    let replace_all = input.replace_all.unwrap_or(false);
    if matches > 1 && !replace_all {
        return Err(ToolError::InvalidInput(format!(
            "oldString matched {} times in {}. Set replaceAll=true or provide a more specific match.",
            matches, resolved.display_path
        )));
    }

    let updated = if replace_all {
        source.text.replace(&old, &replacement)
    } else {
        source.text.replacen(&old, &replacement, 1)
    };
    write_text_file(&resolved.absolute_path, &updated, source.bom || new_bom)?;

    Ok(format!("Updated {}", resolved.display_path))
}

fn workspace_apply_patch(
    runtime: &WorkspaceRuntime,
    input: &WorkspaceApplyPatchInput,
) -> Result<String, ToolError> {
    let operations = parse_patch(&input.patch_text).map_err(|error| {
        ToolError::InvalidInput(format!("apply_patch verification failed: {error}"))
    })?;

    if operations.is_empty() {
        return Err(ToolError::InvalidInput(String::from(
            "apply_patch verification failed: patch was empty",
        )));
    }

    let tools = runtime.tools()?;
    let mut planned = Vec::new();
    let mut touched = HashSet::new();

    for operation in operations {
        match operation {
            PatchOp::Add { path, contents } => {
                let resolved = resolve_workspace_path(tools.root(), &path)?;
                if resolved.absolute_path.exists() {
                    return Err(ToolError::InvalidInput(format!(
                        "apply_patch verification failed: {} already exists",
                        resolved.display_path
                    )));
                }
                ensure_unique_touch(&mut touched, &resolved.display_path)?;

                let (bom, text) = split_bom(&contents);
                planned.push(PlannedChange::Add {
                    path: resolved.absolute_path,
                    display_path: resolved.display_path,
                    contents: text,
                    bom,
                });
            }
            PatchOp::Delete { path } => {
                let resolved = resolve_workspace_path(tools.root(), &path)?;
                read_text_file(&resolved.absolute_path, &resolved.display_path)?;
                ensure_unique_touch(&mut touched, &resolved.display_path)?;

                planned.push(PlannedChange::Delete {
                    path: resolved.absolute_path,
                    display_path: resolved.display_path,
                });
            }
            PatchOp::Update {
                path,
                move_to,
                chunks,
            } => {
                let resolved = resolve_workspace_path(tools.root(), &path)?;
                let source = read_text_file(&resolved.absolute_path, &resolved.display_path)?;
                ensure_unique_touch(&mut touched, &resolved.display_path)?;

                let normalized_chunks = normalize_patch_chunks(&chunks, source.line_ending);
                let updated = if normalized_chunks.is_empty() {
                    source.text.clone()
                } else {
                    apply_chunks(&source.text, &normalized_chunks).map_err(|error| {
                        ToolError::InvalidInput(format!(
                            "apply_patch verification failed for {}: {error}",
                            resolved.display_path
                        ))
                    })?
                };

                if let Some(move_to) = move_to {
                    let destination = resolve_workspace_path(tools.root(), &move_to)?;
                    if destination.absolute_path != resolved.absolute_path
                        && destination.absolute_path.exists()
                    {
                        return Err(ToolError::InvalidInput(format!(
                            "apply_patch verification failed: destination {} already exists",
                            destination.display_path
                        )));
                    }
                    ensure_unique_touch(&mut touched, &destination.display_path)?;

                    planned.push(PlannedChange::Move {
                        source_path: resolved.absolute_path,
                        destination_path: destination.absolute_path,
                        destination_display_path: destination.display_path,
                        contents: updated,
                        bom: source.bom,
                    });
                } else {
                    planned.push(PlannedChange::Update {
                        path: resolved.absolute_path,
                        display_path: resolved.display_path,
                        contents: updated,
                        bom: source.bom,
                    });
                }
            }
        }
    }

    let mut summary = Vec::new();
    for change in &planned {
        match change {
            PlannedChange::Add {
                path,
                display_path,
                contents,
                bom,
            } => {
                write_text_file(path, contents, *bom)?;
                summary.push(format!("A {display_path}"));
            }
            PlannedChange::Update {
                path,
                display_path,
                contents,
                bom,
            } => {
                write_text_file(path, contents, *bom)?;
                summary.push(format!("M {display_path}"));
            }
            PlannedChange::Move {
                source_path,
                destination_path,
                destination_display_path,
                contents,
                bom,
                ..
            } => {
                write_text_file(destination_path, contents, *bom)?;
                fs::remove_file(source_path).map_err(|error| {
                    ToolError::Internal(format!(
                        "Failed to remove {}: {error}",
                        source_path.display()
                    ))
                })?;
                summary.push(format!("M {destination_display_path}"));
            }
            PlannedChange::Delete {
                path, display_path, ..
            } => {
                fs::remove_file(path).map_err(|error| {
                    ToolError::Internal(format!("Failed to remove {}: {error}", path.display()))
                })?;
                summary.push(format!("D {display_path}"));
            }
        }
    }

    Ok(format!(
        "Success. Updated the following files:\n{}",
        summary.join("\n")
    ))
}

fn render_directory(
    path: &Path,
    display_path: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<String, ToolError> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| ToolError::Internal(format!("Failed to read {display_path}: {error}")))?
        .map(|entry| entry.map_err(|error| ToolError::Internal(error.to_string())))
        .collect::<Result<Vec<_>, _>>()?;

    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

    let mut names = Vec::with_capacity(entries.len());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let entry_type = entry
            .file_type()
            .map_err(|error| ToolError::Internal(error.to_string()))?;
        let is_dir = if entry_type.is_symlink() {
            fs::metadata(entry.path())
                .map(|meta| meta.is_dir())
                .unwrap_or(false)
        } else {
            entry_type.is_dir()
        };
        names.push(if is_dir { format!("{name}/") } else { name });
    }

    let offset = offset.unwrap_or(1).max(1);
    let limit = limit.unwrap_or(DEFAULT_READ_LIMIT);
    let start = offset.saturating_sub(1);
    let sliced = names
        .iter()
        .skip(start)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();

    Ok(format!(
        "<path>{display_path}</path>\n<type>directory</type>\n<entries>\n{}\n</entries>",
        sliced.join("\n")
    ))
}

fn render_file(
    path: &Path,
    display_path: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<String, ToolError> {
    let bytes = fs::read(path)
        .map_err(|error| ToolError::Internal(format!("Failed to read {display_path}: {error}")))?;

    if is_binary(&bytes) {
        return Err(ToolError::InvalidInput(format!(
            "`{display_path}` appears to be a binary file. Use workspace-relative paths such as `src/main.rs` for text files."
        )));
    }

    let text = String::from_utf8(bytes).map_err(|_| {
        ToolError::InvalidInput(format!("`{display_path}` is not valid UTF-8 text."))
    })?;
    let offset = offset.unwrap_or(1).max(1);
    let limit = limit.unwrap_or(DEFAULT_READ_LIMIT);
    let start = offset.saturating_sub(1);

    let numbered_lines = text
        .lines()
        .enumerate()
        .skip(start)
        .take(limit)
        .map(|(index, line)| format!("{}: {}", index + 1, truncate_line(line)))
        .collect::<Vec<_>>();

    Ok(format!(
        "<path>{display_path}</path>\n<type>file</type>\n<content>\n{}\n</content>",
        numbered_lines.join("\n")
    ))
}

fn truncate_line(line: &str) -> String {
    let count = line.chars().count();
    if count <= MAX_LINE_LENGTH {
        return line.to_string();
    }

    let truncated = line.chars().take(MAX_LINE_LENGTH).collect::<String>();
    format!("{truncated}{TRUNCATION_SUFFIX}")
}

fn is_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(BINARY_SAMPLE_BYTES)];
    if sample.contains(&0) {
        return true;
    }

    let non_printable = sample
        .iter()
        .filter(|byte| **byte < 9 || (**byte > 13 && **byte < 32))
        .count();

    !sample.is_empty() && non_printable * 10 > sample.len() * 3
}

#[derive(Debug, Clone)]
struct TextFile {
    text: String,
    bom: bool,
    line_ending: &'static str,
}

fn read_text_file(path: &Path, display_path: &str) -> Result<TextFile, ToolError> {
    let bytes = fs::read(path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => ToolError::NotFound(format!(
            "File `{display_path}` was not found. Use workspace-relative paths such as `src/main.rs`."
        )),
        _ => ToolError::Internal(format!("Failed to read {display_path}: {error}")),
    })?;

    decode_text_file(&bytes, display_path)
}

fn read_text_file_if_exists(
    path: &Path,
    display_path: &str,
) -> Result<Option<TextFile>, ToolError> {
    match fs::read(path) {
        Ok(bytes) => decode_text_file(&bytes, display_path).map(Some),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ToolError::Internal(format!(
            "Failed to read {display_path}: {error}"
        ))),
    }
}

fn decode_text_file(bytes: &[u8], display_path: &str) -> Result<TextFile, ToolError> {
    if is_binary(bytes) {
        return Err(ToolError::InvalidInput(format!(
            "`{display_path}` appears to be a binary file."
        )));
    }

    let bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let text_bytes = if bom { &bytes[3..] } else { bytes };
    let text = String::from_utf8(text_bytes.to_vec()).map_err(|_| {
        ToolError::InvalidInput(format!("`{display_path}` is not valid UTF-8 text."))
    })?;

    Ok(TextFile {
        line_ending: detect_line_ending(&text),
        text,
        bom,
    })
}

fn detect_line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

fn normalize_to_line_ending(text: &str, line_ending: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    if line_ending == "\r\n" {
        normalized.replace('\n', "\r\n")
    } else {
        normalized
    }
}

fn split_bom(text: &str) -> (bool, String) {
    if let Some(stripped) = text.strip_prefix('\u{feff}') {
        (true, stripped.to_string())
    } else {
        (false, text.to_string())
    }
}

fn write_text_file(path: &Path, text: &str, bom: bool) -> Result<(), ToolError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ToolError::Internal(format!("Failed to create {}: {error}", parent.display()))
        })?;
    }

    let mut contents = Vec::new();
    if bom {
        contents.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    contents.extend_from_slice(text.as_bytes());

    AtomicFile::new(path, OverwriteBehavior::AllowOverwrite)
        .write(|file| file.write_all(&contents))
        .map_err(|error| {
            ToolError::Internal(format!("Failed to write {}: {error}", path.display()))
        })
}

fn normalize_patch_chunks(chunks: &[PatchChunk], line_ending: &str) -> Vec<PatchChunk> {
    chunks
        .iter()
        .map(|chunk| PatchChunk {
            context: chunk
                .context
                .as_ref()
                .map(|value| normalize_to_line_ending(value, line_ending)),
            old_lines: chunk
                .old_lines
                .iter()
                .map(|value| normalize_to_line_ending(value, line_ending))
                .collect(),
            new_lines: chunk
                .new_lines
                .iter()
                .map(|value| normalize_to_line_ending(value, line_ending))
                .collect(),
            end_of_file: chunk.end_of_file,
        })
        .collect()
}

fn ensure_unique_touch(touched: &mut HashSet<String>, display_path: &str) -> Result<(), ToolError> {
    if touched.insert(display_path.to_string()) {
        Ok(())
    } else {
        Err(ToolError::InvalidInput(format!(
            "apply_patch verification failed: duplicate operation for {display_path}"
        )))
    }
}

enum PlannedChange {
    Add {
        path: std::path::PathBuf,
        display_path: String,
        contents: String,
        bom: bool,
    },
    Update {
        path: std::path::PathBuf,
        display_path: String,
        contents: String,
        bom: bool,
    },
    Move {
        source_path: std::path::PathBuf,
        destination_path: std::path::PathBuf,
        destination_display_path: String,
        contents: String,
        bom: bool,
    },
    Delete {
        path: std::path::PathBuf,
        display_path: String,
    },
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

    fn runtime_for(dir: &TestDir) -> WorkspaceRuntime {
        WorkspaceRuntime::from_root(&dir.path).unwrap()
    }

    #[test]
    fn workspace_read_pages_and_numbers_lines() {
        let dir = TestDir::new("workspace-read-");
        std::fs::write(dir.path.join("notes.txt"), "a\nb\nc\n").unwrap();
        let runtime = runtime_for(&dir);

        let output = workspace_read(
            &runtime,
            &WorkspaceReadInput {
                file_path: String::from("notes.txt"),
                offset: Some(2),
                limit: Some(1),
            },
        )
        .unwrap();

        assert!(output.contains("<path>notes.txt</path>"));
        assert!(output.contains("2: b"));
        assert!(!output.contains("1: a"));
    }

    #[test]
    fn workspace_read_lists_directories() {
        let dir = TestDir::new("workspace-read-");
        std::fs::create_dir_all(dir.path.join("src")).unwrap();
        std::fs::write(dir.path.join("Cargo.toml"), "[package]\n").unwrap();
        let runtime = runtime_for(&dir);

        let output = workspace_read(
            &runtime,
            &WorkspaceReadInput {
                file_path: String::from("."),
                offset: None,
                limit: None,
            },
        )
        .unwrap();

        assert!(output.contains("<type>directory</type>"));
        assert!(output.contains("Cargo.toml"));
        assert!(output.contains("src/"));
    }

    #[test]
    fn workspace_read_truncates_long_lines() {
        let dir = TestDir::new("workspace-read-");
        let long_line = "x".repeat(2100);
        std::fs::write(dir.path.join("long.txt"), format!("{long_line}\n")).unwrap();
        let runtime = runtime_for(&dir);

        let output = workspace_read(
            &runtime,
            &WorkspaceReadInput {
                file_path: String::from("long.txt"),
                offset: None,
                limit: None,
            },
        )
        .unwrap();

        assert!(output.contains(TRUNCATION_SUFFIX));
    }

    #[test]
    fn workspace_read_rejects_binary_files() {
        let dir = TestDir::new("workspace-read-");
        std::fs::write(dir.path.join("data.bin"), [0_u8, 159, 146, 150]).unwrap();
        let runtime = runtime_for(&dir);

        let error = workspace_read(
            &runtime,
            &WorkspaceReadInput {
                file_path: String::from("data.bin"),
                offset: None,
                limit: None,
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("binary file"));
    }

    #[test]
    fn workspace_todowrite_replaces_entire_list() {
        let dir = TestDir::new("workspace-todo-");
        let runtime = runtime_for(&dir);

        let first = workspace_todowrite(
            &runtime,
            WorkspaceTodoWriteInput {
                todos: vec![TodoItem {
                    content: String::from("one"),
                    status: String::from("pending"),
                    priority: String::from("high"),
                }],
            },
        )
        .unwrap();
        assert!(first.contains("one"));

        let second = workspace_todowrite(
            &runtime,
            WorkspaceTodoWriteInput {
                todos: vec![TodoItem {
                    content: String::from("two"),
                    status: String::from("completed"),
                    priority: String::from("low"),
                }],
            },
        )
        .unwrap();
        assert!(second.contains("two"));
        assert!(!second.contains("one"));

        let stored = runtime.tools().unwrap().read_todos().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].content, "two");
    }

    #[tokio::test]
    async fn workspace_edit_replaces_single_match() {
        let dir = TestDir::new("workspace-edit-");
        std::fs::write(dir.path.join("file.txt"), "hello world\n").unwrap();
        let runtime = runtime_for(&dir);

        let result = workspace_edit(
            &runtime,
            WorkspaceEditInput {
                file_path: String::from("file.txt"),
                old_string: String::from("world"),
                new_string: String::from("mars"),
                replace_all: None,
            },
        )
        .await
        .unwrap();

        assert!(result.contains("Updated file.txt"));
        assert_eq!(
            std::fs::read_to_string(dir.path.join("file.txt")).unwrap(),
            "hello mars\n"
        );
    }

    #[tokio::test]
    async fn workspace_edit_fails_when_no_match_exists() {
        let dir = TestDir::new("workspace-edit-");
        std::fs::write(dir.path.join("file.txt"), "hello\n").unwrap();
        let runtime = runtime_for(&dir);

        let error = workspace_edit(
            &runtime,
            WorkspaceEditInput {
                file_path: String::from("file.txt"),
                old_string: String::from("missing"),
                new_string: String::from("mars"),
                replace_all: None,
            },
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("did not match any text"));
    }

    #[tokio::test]
    async fn workspace_edit_fails_on_ambiguous_match_without_replace_all() {
        let dir = TestDir::new("workspace-edit-");
        std::fs::write(dir.path.join("file.txt"), "dup dup\n").unwrap();
        let runtime = runtime_for(&dir);

        let error = workspace_edit(
            &runtime,
            WorkspaceEditInput {
                file_path: String::from("file.txt"),
                old_string: String::from("dup"),
                new_string: String::from("ok"),
                replace_all: Some(false),
            },
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("matched 2 times"));
    }

    #[tokio::test]
    async fn workspace_edit_replace_all_updates_every_match() {
        let dir = TestDir::new("workspace-edit-");
        std::fs::write(dir.path.join("file.txt"), "dup dup\n").unwrap();
        let runtime = runtime_for(&dir);

        workspace_edit(
            &runtime,
            WorkspaceEditInput {
                file_path: String::from("file.txt"),
                old_string: String::from("dup"),
                new_string: String::from("ok"),
                replace_all: Some(true),
            },
        )
        .await
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path.join("file.txt")).unwrap(),
            "ok ok\n"
        );
    }

    #[tokio::test]
    async fn workspace_edit_create_file_mode_writes_new_file() {
        let dir = TestDir::new("workspace-edit-");
        let runtime = runtime_for(&dir);

        let result = workspace_edit(
            &runtime,
            WorkspaceEditInput {
                file_path: String::from("src/new.txt"),
                old_string: String::new(),
                new_string: String::from("created\n"),
                replace_all: None,
            },
        )
        .await
        .unwrap();

        assert!(result.contains("Created src/new.txt"));
        assert_eq!(
            std::fs::read_to_string(dir.path.join("src/new.txt")).unwrap(),
            "created\n"
        );
    }

    #[tokio::test]
    async fn workspace_edit_preserves_existing_line_endings() {
        let dir = TestDir::new("workspace-edit-");
        std::fs::write(dir.path.join("file.txt"), b"hello\r\nworld\r\n").unwrap();
        let runtime = runtime_for(&dir);

        workspace_edit(
            &runtime,
            WorkspaceEditInput {
                file_path: String::from("file.txt"),
                old_string: String::from("hello\nworld\n"),
                new_string: String::from("hi\nplanet\n"),
                replace_all: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(
            std::fs::read(dir.path.join("file.txt")).unwrap(),
            b"hi\r\nplanet\r\n"
        );
    }

    #[tokio::test]
    async fn workspace_apply_patch_supports_add_update_delete_and_move() {
        let dir = TestDir::new("workspace-patch-");
        std::fs::write(dir.path.join("update.txt"), "old\n").unwrap();
        std::fs::write(dir.path.join("delete.txt"), "gone\n").unwrap();
        std::fs::write(dir.path.join("move.txt"), "move-me\n").unwrap();
        let runtime = runtime_for(&dir);

        let patch_text = r"*** Begin Patch
*** Add File: add.txt
+new
*** Update File: update.txt
@@
-old
+updated
*** Delete File: delete.txt
*** Update File: move.txt
*** Move to: moved.txt
@@
-move-me
+moved
*** End Patch";

        let result = workspace_apply_patch(
            &runtime,
            &WorkspaceApplyPatchInput {
                patch_text: patch_text.to_string(),
            },
        )
        .unwrap();

        assert!(result.contains("A add.txt"));
        assert!(result.contains("M update.txt"));
        assert!(result.contains("D delete.txt"));
        assert!(result.contains("M moved.txt"));
        assert_eq!(
            std::fs::read_to_string(dir.path.join("add.txt")).unwrap(),
            "new"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path.join("update.txt")).unwrap(),
            "updated\n"
        );
        assert!(!dir.path.join("delete.txt").exists());
        assert!(!dir.path.join("move.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path.join("moved.txt")).unwrap(),
            "moved\n"
        );
    }

    #[tokio::test]
    async fn workspace_apply_patch_verification_failure_leaves_files_unchanged() {
        let dir = TestDir::new("workspace-patch-");
        let file_path = dir.path.join("update.txt");
        std::fs::write(&file_path, "original\n").unwrap();
        let runtime = runtime_for(&dir);

        let error = workspace_apply_patch(
            &runtime,
            &WorkspaceApplyPatchInput {
                patch_text: String::from(
                    "*** Begin Patch\n*** Update File: update.txt\n@@\n-missing\n+updated\n*** End Patch",
                ),
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("verification failed"));
        assert_eq!(std::fs::read_to_string(file_path).unwrap(), "original\n");
    }

    #[test]
    fn build_registry_registers_only_enabled_workspace_tools() {
        let registry = build_registry(&WorkspaceToolsConfig {
            workspace_read: true,
            workspace_edit: true,
            ..Default::default()
        });

        assert!(registry.contains("workspace_read"));
        assert!(registry.contains("workspace_edit"));
        assert!(!registry.contains("workspace_todowrite"));
        assert!(!registry.contains("workspace_apply_patch"));
    }
}
