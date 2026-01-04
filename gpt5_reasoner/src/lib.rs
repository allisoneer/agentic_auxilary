pub mod client;
pub mod errors;
pub mod optimizer;
pub mod template;
pub mod token;

mod types;
pub use types::*;

pub mod engine;
pub use engine::gpt5_reasoner_impl;

// NEW: logging utilities
mod logging; // not public; used internally via crate::logging

#[cfg(test)]
pub mod test_support;

use universal_tool_core::prelude::*;

#[derive(Clone, Default)]
pub struct Gpt5Reasoner;

#[universal_tool_router(mcp(name = "reasoning_model", version = "0.1.0"))]
impl Gpt5Reasoner {
    #[universal_tool(
        description = "Request assistance from a super smart comrade! This is a great tool to use anytime you want to double check something, or get a second opinion. In addition, it can write full plans for you! The tool will automatically optimize the prompt you send it and combine it with any and all context you pass along. It is best practice to pass as much context as possible and to write descriptions for them that accurately reflect the purpose of the files and/or directories of files (in relation to the prompt). Even though the responses from this tool are from an expert, be sure to look over them with a close eye. Better to have 2 experts than 1, right ;)"
    )]
    pub async fn request(
        &self,
        #[universal_tool_param(
            description = "Prompt to pass in to the request. Be specific and detailed, but attempt to avoid utilize biasing-language. This tool works best with neutral verbage. This allows it to reason over the scope of the problem more efficiently."
        )]
        prompt: String,
        #[universal_tool_param(
            description = r#"List of directories that will be expanded into files. You can choose if you want to walk the directory recurisively or not, if you want to specify a maximum amount of files, and if you want to whitelist/filter by certain file extensions. This can be useful for passing more files that are important to a problem context without having to specify every file path.
        "#
        )]
        directories: Option<Vec<DirectoryMeta>>,
        #[universal_tool_param(
            description = "A list of file paths and their descriptions. File paths can be relative from the directory you were launched from, or full paths from the root of file system."
        )]
        files: Vec<FileMeta>,
        #[universal_tool_param(
            description = r#"Type of the output you desire. An enum with either "plan" or "reasoning" as options. Reasoning is perfect for anytime you need to ask a question or consider something deeply. "plan" is useful for writing fully-fledged implementation plans given a certain desire and context."#
        )]
        prompt_type: PromptType,
        #[universal_tool_param(
            description = "When PromptType::Plan, optional filename to write directly into thoughts/{branch}/plans/. If set, returns the repo-relative path of the created file instead of the content."
        )]
        output_filename: Option<String>,
    ) -> std::result::Result<String, ToolError> {
        gpt5_reasoner_impl(
            prompt,
            files,
            directories,
            None,
            prompt_type,
            output_filename,
        )
        .await
    }
}
