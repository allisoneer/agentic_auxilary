use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

pub fn default_max_files() -> usize {
    1000
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_max_is_1000() {
        assert_eq!(default_max_files(), 1000);
    }
}
