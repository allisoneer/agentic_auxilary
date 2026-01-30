//! Regex-based content search with multiple output modes.

use crate::types::{GrepOutput, OutputMode};
use crate::walker::{self, BUILTIN_IGNORES};
use agentic_tools_core::ToolError;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use regex::Regex;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

/// Configuration for grep search.
#[derive(Debug)]
pub struct GrepConfig {
    /// Root directory to search
    pub root: String,
    /// Regex pattern to search for
    pub pattern: String,
    /// Output mode: files, content, or count
    pub mode: OutputMode,
    /// Include-only glob patterns (files to consider)
    pub include_globs: Vec<String>,
    /// Additional glob patterns to ignore (exclude)
    pub ignore_globs: Vec<String>,
    /// Include hidden files
    pub include_hidden: bool,
    /// Case-insensitive matching
    pub case_insensitive: bool,
    /// Allow patterns to span lines
    pub multiline: bool,
    /// Show line numbers in content mode
    pub line_numbers: bool,
    /// Context lines before and after matches
    pub context: Option<u32>,
    /// Context lines before match
    pub context_before: Option<u32>,
    /// Context lines after match
    pub context_after: Option<u32>,
    /// Search binary files as text
    pub include_binary: bool,
    /// Max results to return (capped at 1000)
    pub head_limit: usize,
    /// Skip the first N results
    pub offset: usize,
}

/// Maximum allowed head_limit to prevent context bloat.
const MAX_HEAD_LIMIT: usize = 1000;

/// Size of buffer for binary detection (8KB).
const BINARY_CHECK_SIZE: usize = 8192;

/// Check if a file appears to be binary by looking for NUL bytes in the first 8KB.
fn is_binary_file(path: &Path) -> std::io::Result<bool> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; BINARY_CHECK_SIZE];
    let bytes_read = file.read(&mut buffer)?;
    Ok(buffer[..bytes_read].contains(&0))
}

/// Build a GlobSet for include patterns.
fn build_include_globset(patterns: &[String]) -> Result<Option<GlobSet>, ToolError> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let g = Glob::new(p).map_err(|e| {
            ToolError::invalid_input(format!("Invalid include glob '{}': {}", p, e))
        })?;
        builder.add(g);
    }
    let gs = builder
        .build()
        .map_err(|e| ToolError::internal(format!("Failed to build include globset: {}", e)))?;
    Ok(Some(gs))
}

/// A match result from searching a file.
#[derive(Debug)]
struct FileMatch {
    /// Relative path to the file
    rel_path: String,
    /// Matched lines with their line numbers (1-indexed)
    lines: Vec<(usize, String)>,
    /// Total number of matches in this file
    match_count: usize,
}

/// Search a single file for matches (line-by-line mode).
fn search_file_lines(
    path: &Path,
    rel_path: &str,
    regex: &Regex,
    cfg: &GrepConfig,
) -> std::io::Result<Option<FileMatch>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut matched_lines: Vec<(usize, String)> = Vec::new();
    let mut match_count = 0;

    // Context tracking
    let ctx_before = cfg.context_before.or(cfg.context).unwrap_or(0) as usize;
    let ctx_after = cfg.context_after.or(cfg.context).unwrap_or(0) as usize;

    // Ring buffer for context before
    let mut before_buffer: Vec<(usize, String)> = Vec::with_capacity(ctx_before);
    let mut after_countdown: usize = 0;
    let mut last_matched_line: usize = 0;

    for (idx, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let line_num = idx + 1; // 1-indexed

        if regex.is_match(&line) {
            match_count += regex.find_iter(&line).count();

            // Add pending context-before lines
            for (ln, content) in before_buffer.drain(..) {
                if matched_lines.is_empty() || ln > last_matched_line {
                    matched_lines.push((ln, content));
                }
            }

            matched_lines.push((line_num, line.clone()));
            last_matched_line = line_num;
            after_countdown = ctx_after;
        } else if after_countdown > 0 {
            // Context after a match
            matched_lines.push((line_num, line.clone()));
            last_matched_line = line_num;
            after_countdown -= 1;
        } else if ctx_before > 0 {
            // Track context before
            if before_buffer.len() >= ctx_before {
                before_buffer.remove(0);
            }
            before_buffer.push((line_num, line));
        }
    }

    if match_count == 0 {
        return Ok(None);
    }

    Ok(Some(FileMatch {
        rel_path: rel_path.to_string(),
        lines: matched_lines,
        match_count,
    }))
}

/// Search a single file for matches (multiline mode).
fn search_file_multiline(
    path: &Path,
    rel_path: &str,
    regex: &Regex,
) -> std::io::Result<Option<FileMatch>> {
    let mut file = File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let matches: Vec<_> = regex.find_iter(&content).collect();
    if matches.is_empty() {
        return Ok(None);
    }

    let match_count = matches.len();

    // For each match, compute the line number and extract the matched text
    let mut matched_lines: Vec<(usize, String)> = Vec::new();
    for m in &matches {
        let start = m.start();
        // Count newlines before match to get line number
        let line_num = content[..start].matches('\n').count() + 1;
        // Get the matched text (may span multiple lines)
        let matched_text = m.as_str().replace('\n', "\\n");
        matched_lines.push((line_num, matched_text));
    }

    Ok(Some(FileMatch {
        rel_path: rel_path.to_string(),
        lines: matched_lines,
        match_count,
    }))
}

/// Run grep search with the given configuration.
pub fn run(cfg: GrepConfig) -> Result<GrepOutput, ToolError> {
    // Validate root path
    let root_path = Path::new(&cfg.root);
    if !root_path.exists() {
        return Err(ToolError::invalid_input(format!(
            "Path does not exist: {}",
            cfg.root
        )));
    }

    // Build regex
    let mut rb = regex::RegexBuilder::new(&cfg.pattern);
    rb.case_insensitive(cfg.case_insensitive);
    if cfg.multiline {
        rb.multi_line(true).dot_matches_new_line(true);
    }
    let regex = rb
        .build()
        .map_err(|e| ToolError::invalid_input(format!("Invalid regex: {}", e)))?;

    // Build include globset
    let include_gs = build_include_globset(&cfg.include_globs)?;

    // Build ignore globset
    let ignore_gs = walker::build_ignore_globset(&cfg.ignore_globs)?;

    // Cap head_limit
    let head_limit = cfg.head_limit.min(MAX_HEAD_LIMIT);

    let mut warnings: Vec<String> = Vec::new();
    let mut all_matches: Vec<FileMatch> = Vec::new();
    let mut binary_skipped = 0usize;

    // Handle single file case
    if root_path.is_file() {
        let rel_path = root_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| cfg.root.clone());

        // Check binary
        if !cfg.include_binary {
            match is_binary_file(root_path) {
                Ok(true) => {
                    binary_skipped = 1;
                }
                Ok(false) => {
                    let result = if cfg.multiline {
                        search_file_multiline(root_path, &rel_path, &regex)
                    } else {
                        search_file_lines(root_path, &rel_path, &regex, &cfg)
                    };
                    if let Ok(Some(m)) = result {
                        all_matches.push(m);
                    }
                }
                Err(e) => {
                    warnings.push(format!("Could not read {}: {}", rel_path, e));
                }
            }
        } else {
            let result = if cfg.multiline {
                search_file_multiline(root_path, &rel_path, &regex)
            } else {
                search_file_lines(root_path, &rel_path, &regex, &cfg)
            };
            match result {
                Ok(Some(m)) => all_matches.push(m),
                Ok(None) => {}
                Err(e) => warnings.push(format!("Could not read {}: {}", rel_path, e)),
            }
        }
    } else {
        // Directory traversal
        let mut builder = WalkBuilder::new(root_path);
        builder.hidden(!cfg.include_hidden);
        builder.git_ignore(true);
        builder.git_global(true);
        builder.git_exclude(true);
        builder.parents(false);
        builder.follow_links(false);

        // Apply custom ignore filter
        let root_clone = root_path.to_path_buf();
        let gs_clone = ignore_gs.clone();
        builder.filter_entry(move |entry| {
            let rel = entry
                .path()
                .strip_prefix(&root_clone)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if rel.is_empty() {
                return true;
            }
            !gs_clone.is_match(&rel)
        });

        for result in builder.build() {
            match result {
                Ok(entry) => {
                    let path = entry.path();

                    // Skip directories
                    if path.is_dir() {
                        continue;
                    }

                    let rel_path = path
                        .strip_prefix(root_path)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| path.to_string_lossy().to_string());

                    // Double-check against ignore patterns
                    if ignore_gs.is_match(&rel_path) {
                        continue;
                    }

                    // Check against builtin ignores
                    let matches_builtin = BUILTIN_IGNORES.iter().any(|pattern| {
                        if let Ok(g) = Glob::new(pattern) {
                            g.compile_matcher().is_match(&rel_path)
                        } else {
                            false
                        }
                    });
                    if matches_builtin {
                        continue;
                    }

                    // Check include patterns
                    if let Some(ref inc_gs) = include_gs
                        && !inc_gs.is_match(&rel_path)
                    {
                        continue;
                    }

                    // Check binary
                    if !cfg.include_binary {
                        match is_binary_file(path) {
                            Ok(true) => {
                                binary_skipped += 1;
                                continue;
                            }
                            Ok(false) => {}
                            Err(_) => continue,
                        }
                    }

                    // Search the file
                    let search_result = if cfg.multiline {
                        search_file_multiline(path, &rel_path, &regex)
                    } else {
                        search_file_lines(path, &rel_path, &regex, &cfg)
                    };

                    match search_result {
                        Ok(Some(m)) => all_matches.push(m),
                        Ok(None) => {}
                        Err(e) => {
                            warnings.push(format!("Could not read {}: {}", rel_path, e));
                        }
                    }
                }
                Err(e) => {
                    warnings.push(format!("Walk error: {}", e));
                }
            }
        }
    }

    // Add binary skip warning if applicable
    if binary_skipped > 0 {
        warnings.push(format!(
            "{} binary file{} skipped (use include_binary=true to search)",
            binary_skipped,
            if binary_skipped == 1 { "" } else { "s" }
        ));
    }

    // Format output based on mode
    let (lines, summary, total_count) = match cfg.mode {
        OutputMode::Files => {
            // Unique file paths
            let mut seen: HashSet<String> = HashSet::new();
            let mut file_paths: Vec<String> = Vec::new();
            for m in &all_matches {
                if seen.insert(m.rel_path.clone()) {
                    file_paths.push(m.rel_path.clone());
                }
            }
            let total = file_paths.len();
            (file_paths, None, total)
        }
        OutputMode::Content => {
            // path:line: content format
            let mut output_lines: Vec<String> = Vec::new();
            for m in &all_matches {
                for (line_num, content) in &m.lines {
                    if cfg.line_numbers {
                        output_lines.push(format!("{}:{}: {}", m.rel_path, line_num, content));
                    } else {
                        output_lines.push(format!("{}: {}", m.rel_path, content));
                    }
                }
            }
            let total = output_lines.len();
            (output_lines, None, total)
        }
        OutputMode::Count => {
            // Total match count
            let total: usize = all_matches.iter().map(|m| m.match_count).sum();
            let summary = format!("Total matches: {}", total);
            (vec![], Some(summary), total)
        }
    };

    // Apply pagination
    let offset = cfg.offset;
    let paginated: Vec<String> = lines.into_iter().skip(offset).take(head_limit).collect();
    let has_more = total_count > offset + paginated.len();

    Ok(GrepOutput {
        root: cfg.root,
        mode: cfg.mode,
        lines: paginated,
        has_more,
        warnings,
        summary,
    })
}
