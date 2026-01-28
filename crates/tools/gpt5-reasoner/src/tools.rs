//! Tool wrappers for gpt5_reasoner using agentic-tools-core.
//!
//! Provides the `request` tool that wraps the reasoning model functionality.

use crate::{DirectoryMeta, FileMeta, PromptType, gpt5_reasoner_impl};
use agentic_tools_core::{Tool, ToolContext, ToolError, ToolRegistry};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::Deserialize;

// ============================================================================
// Request Tool
// ============================================================================

/// Input for the reasoning model request tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RequestInput {
    /// Prompt to pass in to the request. Be specific and detailed, but attempt
    /// to avoid using biasing language. This tool works best with neutral verbiage.
    /// This allows it to reason over the scope of the problem more efficiently.
    pub prompt: String,

    /// List of directories that will be expanded into files. You can choose if you
    /// want to walk the directory recursively or not, if you want to specify a
    /// maximum amount of files, and if you want to whitelist/filter by certain file
    /// extensions. This can be useful for passing more files that are important to
    /// a problem context without having to specify every file path.
    #[serde(default)]
    pub directories: Option<Vec<DirectoryMeta>>,

    /// A list of file paths and their descriptions. File paths can be relative from
    /// the directory you were launched from, or full paths from the root of file system.
    pub files: Vec<FileMeta>,

    /// Type of the output you desire. An enum with either "plan" or "reasoning" as
    /// options. Reasoning is perfect for anytime you need to ask a question or consider
    /// something deeply. "plan" is useful for writing fully-fledged implementation
    /// plans given a certain desire and context.
    pub prompt_type: PromptType,

    /// When PromptType::Plan, optional filename to write directly into
    /// thoughts/{branch}/plans/. If set, returns the repo-relative path of the
    /// created file instead of the content.
    #[serde(default)]
    pub output_filename: Option<String>,
}

/// Tool for requesting assistance from the reasoning model.
#[derive(Clone)]
pub struct RequestTool;

impl Tool for RequestTool {
    type Input = RequestInput;
    type Output = String;
    const NAME: &'static str = "ask_reasoning_model";
    const DESCRIPTION: &'static str = "Request assistance from a super smart comrade! This is a great tool to use anytime you want to double check something, or get a second opinion. In addition, it can write full plans for you! The tool will automatically optimize the prompt you send it and combine it with any and all context you pass along. It is best practice to pass as much context as possible and to write descriptions for them that accurately reflect the purpose of the files and/or directories of files (in relation to the prompt). Even though the responses from this tool are from an expert, be sure to look over them with a close eye. Better to have 2 experts than 1, right ;)";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            gpt5_reasoner_impl(
                input.prompt,
                input.files,
                input.directories,
                None,
                input.prompt_type,
                input.output_filename,
            )
            .await
        })
    }
}

// ============================================================================
// Registry Builder
// ============================================================================

/// Build a ToolRegistry containing all gpt5_reasoner tools.
pub fn build_registry() -> ToolRegistry {
    ToolRegistry::builder()
        .register::<RequestTool, ()>(RequestTool)
        .finish()
}
