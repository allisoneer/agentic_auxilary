//! CLI utilities for the Universal Tool Framework
//!
//! This module provides utilities for CLI applications including output formatting,
//! progress reporting, and interactive prompts.

use serde::Serialize;
use std::fmt;

// Re-export clap and related types so users don't need to depend on them
pub use clap;
pub use clap_complete;

/// Output format options for CLI commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text output
    Text,
    /// JSON output
    Json,
    /// YAML output
    Yaml,
    /// Table output
    Table,
    /// CSV output
    Csv,
}

impl OutputFormat {
    /// Parse output format from string
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "text" | "human" => Ok(OutputFormat::Text),
            "json" => Ok(OutputFormat::Json),
            "yaml" | "yml" => Ok(OutputFormat::Yaml),
            "table" => Ok(OutputFormat::Table),
            "csv" => Ok(OutputFormat::Csv),
            _ => Err(format!("Unknown output format: {s}")),
        }
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Text => write!(f, "text"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Yaml => write!(f, "yaml"),
            OutputFormat::Table => write!(f, "table"),
            OutputFormat::Csv => write!(f, "csv"),
        }
    }
}

/// Trait for types that can be formatted for CLI output
pub trait CliFormatter: Serialize {
    /// Format as human-readable text
    fn format_text(&self) -> String {
        // Default implementation using JSON pretty print
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "Failed to format".to_string())
    }

    /// Format as a table (returns rows of data)
    fn format_table(&self) -> Vec<Vec<String>> {
        // Default implementation returns empty table
        vec![]
    }

    /// Format as CSV rows
    fn format_csv(&self) -> Vec<Vec<String>> {
        // Default to table format
        self.format_table()
    }

    /// Format output based on the specified format
    fn format_output(&self, format: OutputFormat) -> Result<String, crate::error::ToolError> {
        match format {
            OutputFormat::Text => Ok(self.format_text()),
            OutputFormat::Json => serde_json::to_string_pretty(self).map_err(|e| {
                crate::error::ToolError::new(
                    crate::error::ErrorCode::SerializationError,
                    format!("Failed to serialize to JSON: {e}"),
                )
            }),
            OutputFormat::Yaml => serde_yaml::to_string(self).map_err(|e| {
                crate::error::ToolError::new(
                    crate::error::ErrorCode::SerializationError,
                    format!("Failed to serialize to YAML: {e}"),
                )
            }),
            OutputFormat::Table => {
                use tabled::builder::Builder;
                let rows = self.format_table();
                if rows.is_empty() {
                    Ok("No data to display".to_string())
                } else {
                    let mut builder = Builder::default();
                    for row in rows {
                        builder.push_record(row);
                    }
                    Ok(builder.build().to_string())
                }
            }
            OutputFormat::Csv => {
                use std::io::Cursor;
                let mut buffer = Cursor::new(Vec::new());
                {
                    let mut writer = csv::Writer::from_writer(&mut buffer);

                    for row in self.format_csv() {
                        writer.write_record(&row).map_err(|e| {
                            crate::error::ToolError::new(
                                crate::error::ErrorCode::SerializationError,
                                format!("Failed to write CSV: {e}"),
                            )
                        })?;
                    }

                    writer.flush().map_err(|e| {
                        crate::error::ToolError::new(
                            crate::error::ErrorCode::SerializationError,
                            format!("Failed to flush CSV writer: {e}"),
                        )
                    })?;
                } // writer is dropped here, releasing the borrow

                String::from_utf8(buffer.into_inner()).map_err(|e| {
                    crate::error::ToolError::new(
                        crate::error::ErrorCode::SerializationError,
                        format!("Failed to convert CSV to string: {e}"),
                    )
                })
            }
        }
    }
}

/// Progress reporter for long-running operations
pub struct ProgressReporter {
    _style: ProgressStyle,
    bar: Option<indicatif::ProgressBar>,
}

/// Progress indicator style
#[derive(Debug, Clone, Copy)]
pub enum ProgressStyle {
    /// Progress bar with percentage
    Bar,
    /// Spinning indicator
    Spinner,
    /// Animated dots
    Dots,
    /// No progress indicator
    None,
}

impl ProgressReporter {
    /// Create a new progress reporter
    pub fn new(style: ProgressStyle, total: Option<u64>) -> Self {
        use indicatif::{ProgressBar, ProgressStyle as IndicatifStyle};

        let bar = match style {
            ProgressStyle::Bar => {
                let pb = if let Some(total) = total {
                    ProgressBar::new(total)
                } else {
                    ProgressBar::new_spinner()
                };
                pb.set_style(
                    IndicatifStyle::default_bar()
                        .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                        .unwrap()
                        .progress_chars("#>-"),
                );
                Some(pb)
            }
            ProgressStyle::Spinner => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    IndicatifStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                Some(pb)
            }
            ProgressStyle::Dots => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    IndicatifStyle::default_spinner()
                        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );
                Some(pb)
            }
            ProgressStyle::None => None,
        };

        if let Some(ref pb) = bar {
            pb.enable_steady_tick(std::time::Duration::from_millis(100));
        }

        Self { _style: style, bar }
    }

    /// Update progress
    pub fn set_progress(&self, current: u64) {
        if let Some(ref bar) = self.bar {
            bar.set_position(current);
        }
    }

    /// Update message
    pub fn set_message(&self, message: impl Into<String>) {
        if let Some(ref bar) = self.bar {
            bar.set_message(message.into());
        }
    }

    /// Increment progress by 1
    pub fn inc(&self, delta: u64) {
        if let Some(ref bar) = self.bar {
            bar.inc(delta);
        }
    }

    /// Finish with a message
    pub fn finish_with_message(&self, message: impl Into<String>) {
        if let Some(ref bar) = self.bar {
            bar.finish_with_message(message.into());
        }
    }

    /// Finish and clear
    pub fn finish_and_clear(&self) {
        if let Some(ref bar) = self.bar {
            bar.finish_and_clear();
        }
    }
}

/// Interactive prompt utilities
pub mod interactive {
    use crate::error::{ErrorCode, ToolError};

    /// Prompt for text input
    pub fn input(prompt: &str) -> Result<String, ToolError> {
        dialoguer::Input::new()
            .with_prompt(prompt)
            .interact_text()
            .map_err(|e| ToolError::new(ErrorCode::IoError, format!("Failed to read input: {e}")))
    }

    /// Prompt for text input with validation
    pub fn input_with_validation<F>(prompt: &str, validator: F) -> Result<String, ToolError>
    where
        F: Fn(&String) -> Result<(), String> + Clone,
    {
        dialoguer::Input::new()
            .with_prompt(prompt)
            .validate_with(validator)
            .interact_text()
            .map_err(|e| ToolError::new(ErrorCode::IoError, format!("Failed to read input: {e}")))
    }

    /// Prompt for selection from a list
    pub fn select<T>(prompt: &str, items: &[T]) -> Result<usize, ToolError>
    where
        T: ToString,
    {
        dialoguer::Select::new()
            .with_prompt(prompt)
            .items(items)
            .interact()
            .map_err(|e| {
                ToolError::new(
                    ErrorCode::IoError,
                    format!("Failed to read selection: {e}"),
                )
            })
    }

    /// Prompt for multiple selections
    pub fn multi_select<T>(prompt: &str, items: &[T]) -> Result<Vec<usize>, ToolError>
    where
        T: ToString,
    {
        dialoguer::MultiSelect::new()
            .with_prompt(prompt)
            .items(items)
            .interact()
            .map_err(|e| {
                ToolError::new(
                    ErrorCode::IoError,
                    format!("Failed to read selections: {e}"),
                )
            })
    }

    /// Prompt for confirmation
    pub fn confirm(prompt: &str, default: bool) -> Result<bool, ToolError> {
        dialoguer::Confirm::new()
            .with_prompt(prompt)
            .default(default)
            .interact()
            .map_err(|e| {
                ToolError::new(
                    ErrorCode::IoError,
                    format!("Failed to read confirmation: {e}"),
                )
            })
    }
}

/// Check if stdin is a terminal (not piped)
pub fn is_stdin_tty() -> bool {
    atty::is(atty::Stream::Stdin)
}

/// Check if stdout is a terminal (not piped)
pub fn is_stdout_tty() -> bool {
    atty::is(atty::Stream::Stdout)
}

/// Read from stdin if available
pub fn read_stdin() -> Result<Option<String>, crate::error::ToolError> {
    use std::io::Read;

    if !is_stdin_tty() {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer).map_err(|e| {
            crate::error::ToolError::new(
                crate::error::ErrorCode::IoError,
                format!("Failed to read from stdin: {e}"),
            )
        })?;
        Ok(Some(buffer))
    } else {
        Ok(None)
    }
}
