//! Advanced CLI example demonstrating UTF's sophisticated CLI generation capabilities
//!
//! This example showcases:
//! - Multiple output formats (JSON, YAML, table, text)
//! - Progress indicators for long-running operations
//! - Shell completion generation
//! - Vec<T> parameter types
//! - Complex parameter types and validation
//! - Interactive mode
//! - Piping support

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use universal_tool_core::cli::clap_complete;
use universal_tool_core::prelude::*;
use universal_tool_core::schemars;

/// Data analysis tools implementation
struct DataTools;

// Implementation moved to main impl block below

/// Analysis result structure with table formatting support
#[derive(Debug, Serialize, Deserialize)]
struct AnalysisResult {
    files_processed: usize,
    total_lines: usize,
    total_size_bytes: u64,
    processing_time_ms: u64,
    file_types: Vec<FileTypeInfo>,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileTypeInfo {
    extension: String,
    count: usize,
    lines: usize,
}

impl CliFormatter for AnalysisResult {
    fn format_text(&self) -> String {
        format!(
            "Analysis Complete!\n\
             Files processed: {}\n\
             Total lines: {}\n\
             Total size: {} bytes\n\
             Processing time: {}ms\n\
             Timestamp: {}",
            self.files_processed,
            self.total_lines,
            self.total_size_bytes,
            self.processing_time_ms,
            self.timestamp.to_rfc3339()
        )
    }

    fn format_table(&self) -> Vec<Vec<String>> {
        let mut rows = vec![
            vec!["Metric".to_string(), "Value".to_string()],
            vec![
                "Files Processed".to_string(),
                self.files_processed.to_string(),
            ],
            vec!["Total Lines".to_string(), self.total_lines.to_string()],
            vec![
                "Total Size".to_string(),
                format!("{} bytes", self.total_size_bytes),
            ],
            vec![
                "Processing Time".to_string(),
                format!("{}ms", self.processing_time_ms),
            ],
            vec!["Timestamp".to_string(), self.timestamp.to_rfc3339()],
        ];

        if !self.file_types.is_empty() {
            rows.push(vec!["".to_string(), "".to_string()]);
            rows.push(
                vec![
                    "File Type".to_string(),
                    "Count".to_string(),
                    "Lines".to_string(),
                ]
                .into_iter()
                .map(String::from)
                .collect(),
            );
            for ft in &self.file_types {
                rows.push(vec![
                    ft.extension.clone(),
                    ft.count.to_string(),
                    ft.lines.to_string(),
                ]);
            }
        }

        rows
    }
}

/// Batch processing result
#[derive(Debug, Serialize, Deserialize)]
struct BatchResult {
    successful: Vec<String>,
    failed: Vec<(String, String)>,
    total_time_ms: u64,
}

/// Configuration processing result
#[derive(Debug, Serialize, Deserialize)]
struct ConfigResult {
    config_count: usize,
    config_keys: Vec<String>,
    settings_sum: i32,
    sample_config: Vec<String>,
}

/// Custom filter configuration
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct FilterConfig {
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
    min_size: Option<u64>,
    max_size: Option<u64>,
}

/// Filter result
#[derive(Debug, Serialize, Deserialize)]
struct FilterResult {
    matched_count: usize,
    matched_files: Vec<String>,
    filter_summary: String,
}

impl CliFormatter for FilterResult {
    fn format_text(&self) -> String {
        format!(
            "Filter Results:\n\
             Matched files: {}\n\
             Filter: {}\n\n\
             Files:\n{}",
            self.matched_count,
            self.filter_summary,
            self.matched_files
                .iter()
                .map(|f| format!("  - {}", f))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

impl CliFormatter for BatchResult {
    fn format_text(&self) -> String {
        let mut output = format!(
            "Batch Processing Complete!\n\
             Successful: {}\n\
             Failed: {}\n\
             Total time: {}ms\n",
            self.successful.len(),
            self.failed.len(),
            self.total_time_ms
        );

        if !self.failed.is_empty() {
            output.push_str("\nFailed items:\n");
            for (item, error) in &self.failed {
                output.push_str(&format!("  - {}: {}\n", item, error));
            }
        }

        output
    }

    fn format_table(&self) -> Vec<Vec<String>> {
        let mut rows = vec![
            vec!["Status".to_string(), "Count".to_string()],
            vec!["Successful".to_string(), self.successful.len().to_string()],
            vec!["Failed".to_string(), self.failed.len().to_string()],
            vec![
                "Total Time".to_string(),
                format!("{}ms", self.total_time_ms),
            ],
        ];

        if !self.failed.is_empty() {
            rows.push(vec!["".to_string(), "".to_string()]);
            rows.push(vec!["Failed Item".to_string(), "Error".to_string()]);
            for (item, error) in &self.failed {
                rows.push(vec![item.clone(), error.clone()]);
            }
        }

        rows
    }
}

impl CliFormatter for ConfigResult {
    fn format_text(&self) -> String {
        let mut output = format!(
            "Configuration Processing Complete\n\
             Config entries: {}\n\
             Settings sum: {}\n\
             Config keys: {}\n",
            self.config_count,
            self.settings_sum,
            self.config_keys.join(", ")
        );

        if !self.sample_config.is_empty() {
            output.push_str("\nSample config:\n");
            for item in &self.sample_config {
                output.push_str(&format!("  - {}\n", item));
            }
        }

        output
    }

    fn format_table(&self) -> Vec<Vec<String>> {
        let mut rows = vec![
            vec!["Metric".to_string(), "Value".to_string()],
            vec!["Config Entries".to_string(), self.config_count.to_string()],
            vec!["Settings Sum".to_string(), self.settings_sum.to_string()],
            vec!["Config Keys".to_string(), self.config_keys.join(", ")],
        ];

        if !self.sample_config.is_empty() {
            rows.push(vec!["".to_string(), "".to_string()]);
            rows.push(vec!["Sample Config".to_string(), "".to_string()]);
            for item in &self.sample_config {
                rows.push(vec!["".to_string(), item.clone()]);
            }
        }

        rows
    }
}

#[universal_tool_router(cli(
    name = "data-tools",
    description = "Advanced data processing and analysis tools"
))]
impl DataTools {
    fn new() -> Self {
        Self
    }

    /// Analyze multiple files with progress tracking
    ///
    /// Analyzes files and directories for statistics with progress bar support.
    /// Supports stdin input and multiple output formats.
    #[universal_tool(
        description = "Analyze multiple files with progress tracking",
        cli(name = "analyze")
    )]
    pub async fn analyze_files(
        &self,
        #[universal_tool_param(
            description = "Files or directories to analyze (accepts multiple values)"
        )]
        paths: Vec<String>,
        #[universal_tool_param(description = "Filter by file extensions")] extensions: Vec<String>,
        #[universal_tool_param(description = "Show progress bar during analysis")]
        show_progress: bool,
    ) -> Result<AnalysisResult, ToolError> {
        let start = std::time::Instant::now();

        // Simulate file analysis with progress
        let total_files = paths.len();
        // TODO: ProgressReporter should be injected by the framework based on cli(progress_style = "bar")
        // For now, we'll simulate without actual progress reporting

        let mut files_processed = 0;
        let mut total_lines = 0;
        let mut total_size_bytes = 0;
        let mut file_types = std::collections::HashMap::new();

        for (_i, path) in paths.iter().enumerate() {
            // Progress would be reported here if injected by framework

            // Simulate processing
            tokio::time::sleep(Duration::from_millis(100)).await;

            // Mock data
            files_processed += 1;
            total_lines += 100;
            total_size_bytes += 1024;

            let ext = path.split('.').last().unwrap_or("txt").to_string();
            let entry = file_types.entry(ext.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += 100;
        }

        // Progress reporting would finish here

        Ok(AnalysisResult {
            files_processed,
            total_lines,
            total_size_bytes,
            processing_time_ms: start.elapsed().as_millis() as u64,
            file_types: file_types
                .into_iter()
                .map(|(ext, (count, lines))| FileTypeInfo {
                    extension: ext,
                    count,
                    lines,
                })
                .collect(),
            timestamp: Utc::now(),
        })
    }

    /// Process items in batch with progress tracking
    ///
    /// Processes multiple items in batch mode with spinner progress indicator.
    /// Prompts for confirmation before processing.
    #[universal_tool(
        description = "Process items in batch with progress tracking",
        cli(name = "batch")
    )]
    pub async fn batch_process(
        &self,
        #[universal_tool_param(description = "Items to process (accepts multiple values)")]
        items: Vec<String>,
        #[universal_tool_param(description = "Processing mode: fast, normal, or thorough")]
        mode: String,
        #[universal_tool_param(description = "Fail on first error")] fail_fast: bool,
    ) -> Result<BatchResult, ToolError> {
        let start = std::time::Instant::now();
        // TODO: ProgressReporter should be injected by framework

        let mut successful = Vec::new();
        let mut failed = Vec::new();

        for item in items {
            // Progress would be reported here if injected by framework

            // Simulate processing with potential failures
            tokio::time::sleep(Duration::from_millis(200)).await;

            if item.contains("error") {
                failed.push((item.clone(), "Simulated error".to_string()));
                if fail_fast {
                    return Err(ToolError::new(
                        ErrorCode::ExecutionFailed,
                        format!("Failed to process: {}", item),
                    ));
                }
            } else {
                successful.push(item);
            }
        }

        // Progress reporting would finish here

        Ok(BatchResult {
            successful,
            failed,
            total_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Generate shell completions (hidden command)
    ///
    /// Generates shell completion scripts for bash, zsh, fish, or powershell.
    #[universal_tool(
        description = "Generate shell completions",
        cli(name = "completions", hidden = true)
    )]
    pub async fn generate_shell_completions(
        &self,
        #[universal_tool_param(description = "Shell type: bash, zsh, fish, or powershell")]
        shell: String,
    ) -> Result<String, ToolError> {
        let shell = match shell.as_str() {
            "bash" => clap_complete::Shell::Bash,
            "zsh" => clap_complete::Shell::Zsh,
            "fish" => clap_complete::Shell::Fish,
            "powershell" => clap_complete::Shell::PowerShell,
            _ => return Err("Invalid shell type".into()),
        };

        Ok(self.generate_completions(shell))
    }

    /// Process data with custom configuration
    ///
    /// Processes data using key-value configuration options.
    /// Supports text, JSON, and YAML output formats.
    #[universal_tool(
        description = "Process data with custom configuration",
        cli(name = "process-config")
    )]
    pub async fn process_with_config(
        &self,
        #[universal_tool_param(description = "Configuration options as key=value pairs")]
        config: std::collections::HashMap<String, String>,
        #[universal_tool_param(description = "Numeric settings (use --settings key=value)")]
        settings: std::collections::HashMap<String, i32>,
    ) -> Result<ConfigResult, ToolError> {
        Ok(ConfigResult {
            config_count: config.len(),
            config_keys: config.keys().cloned().collect(),
            settings_sum: settings.values().sum(),
            sample_config: config
                .iter()
                .take(3)
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect(),
        })
    }

    /// Filter files with custom configuration
    ///
    /// Filters files based on custom criteria including patterns and size limits.
    /// Supports text and JSON output formats.
    #[universal_tool(
        description = "Filter files with custom configuration",
        cli(name = "filter")
    )]
    pub async fn filter_files(
        &self,
        #[universal_tool_param(
            description = "Filter configuration with include/exclude patterns and size limits"
        )]
        filter: FilterConfig,
        #[universal_tool_param(description = "Directories to search (accepts multiple values)")]
        directories: Vec<String>,
    ) -> Result<FilterResult, ToolError> {
        // Simulate filtering
        let matched_files = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "tests/integration.rs".to_string(),
        ];

        Ok(FilterResult {
            matched_count: matched_files.len(),
            matched_files,
            filter_summary: format!(
                "Include: {}, Exclude: {}, Size: {:?}-{:?}",
                filter.include_patterns.join(", "),
                filter.exclude_patterns.join(", "),
                filter.min_size,
                filter.max_size
            ),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tools = DataTools::new();

    // Create the CLI app with output format handling
    let app = tools.create_cli_command();
    let matches = app.get_matches();

    // Check global flags
    let quiet = matches.get_flag("quiet");
    let verbose = matches.get_count("verbose");

    if verbose > 0 && !quiet {
        eprintln!("Running with verbosity level: {}", verbose);
    }

    // Execute the tool - output formatting is handled inside execute_cli
    match tools.execute_cli(matches).await {
        Ok(()) => Ok(()),
        Err(e) => {
            if !quiet {
                eprintln!("Error: {}", e);
            }
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_analyze_files() {
        let tools = DataTools::new();
        let result = tools
            .analyze_files(
                vec!["test.txt".to_string(), "data.json".to_string()],
                vec![],
                false,
            )
            .await
            .unwrap();

        assert_eq!(result.files_processed, 2);
        assert!(result.total_lines > 0);
    }

    #[tokio::test]
    async fn test_batch_process() {
        let tools = DataTools::new();
        let result = tools
            .batch_process(
                vec!["item1".to_string(), "item2".to_string()],
                "fast".to_string(),
                false,
            )
            .await
            .unwrap();

        assert_eq!(result.successful.len(), 2);
        assert_eq!(result.failed.len(), 0);
    }

    #[test]
    fn test_output_formatting() {
        let result = AnalysisResult {
            files_processed: 10,
            total_lines: 1000,
            total_size_bytes: 10240,
            processing_time_ms: 150,
            file_types: vec![
                FileTypeInfo {
                    extension: "rs".to_string(),
                    count: 5,
                    lines: 500,
                },
                FileTypeInfo {
                    extension: "toml".to_string(),
                    count: 5,
                    lines: 500,
                },
            ],
            timestamp: Utc::now(),
        };

        // Test text format
        let text = result.format_text();
        assert!(text.contains("Files processed: 10"));

        // Test table format
        let table = result.format_table();
        assert!(table.len() > 5);

        // Test JSON format
        let json = result.format_output(OutputFormat::Json).unwrap();
        assert!(json.contains("\"files_processed\": 10"));
    }
}
