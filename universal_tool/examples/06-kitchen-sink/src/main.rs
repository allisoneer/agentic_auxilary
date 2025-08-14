use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ::tower_http::cors::CorsLayer;
use ::tower_http::trace::TraceLayer;
use chrono::{DateTime, Utc};
use clap::Parser;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use universal_tool_core::prelude::*;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct FileInfo {
    path: String,
    size: u64,
    modified: DateTime<Utc>,
    is_directory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct DirectoryListing {
    path: String,
    files: Vec<FileInfo>,
    total_size: u64,
    file_count: usize,
    dir_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct FileContent {
    path: String,
    content: String,
    encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct SearchResult {
    path: String,
    line_number: usize,
    line_content: String,
    match_start: usize,
    match_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct SearchResults {
    query: String,
    results: Vec<SearchResult>,
    total_matches: usize,
    files_searched: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct FileStats {
    total_files: usize,
    total_directories: usize,
    total_size: u64,
    largest_file: Option<FileInfo>,
    most_recent_file: Option<FileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CreateFileRequest {
    path: String,
    content: String,
    overwrite: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CopyMoveRequest {
    source: String,
    destination: String,
    overwrite: Option<bool>,
}

#[derive(Clone)]
struct FileManager {
    operation_log: Arc<Mutex<Vec<String>>>,
}

impl FileManager {
    fn new() -> Self {
        Self {
            operation_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn log_operation(&self, operation: String) {
        if let Ok(mut log) = self.operation_log.lock() {
            log.push(format!(
                "{}: {}",
                Utc::now().format("%Y-%m-%d %H:%M:%S"),
                operation
            ));
        }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        if path.starts_with("~") {
            if let Some(home) = home::home_dir() {
                return home.join(&path[2..]);
            }
        }
        PathBuf::from(path)
    }
}

#[universal_tool_router(
    cli(name = "filemanager"),
    rest(prefix = "/api/v1"),
    mcp(name = "file-manager")
)]
impl FileManager {
    #[universal_tool(
        description = "List files in a directory",
        cli(name = "ls"),
        rest(method = "GET", path = "/files")
    )]
    async fn list_files(
        &self,
        #[universal_tool_param(description = "Directory path", source = "query")] path: String,
        #[universal_tool_param(description = "Include hidden files", source = "query")]
        include_hidden: Option<bool>,
        #[universal_tool_param(description = "Recursive listing", source = "query")]
        recursive: Option<bool>,
    ) -> Result<DirectoryListing, ToolError> {
        let resolved_path = self.resolve_path(&path);

        if !resolved_path.exists() {
            return Err(ToolError::not_found(format!("Path not found: {path}")));
        }

        if !resolved_path.is_dir() {
            return Err(ToolError::invalid_input(format!(
                "Not a directory: {path}"
            )));
        }

        let include_hidden = include_hidden.unwrap_or(false);
        let recursive = recursive.unwrap_or(false);

        let mut files = Vec::new();
        let mut total_size = 0u64;
        let mut file_count = 0;
        let mut dir_count = 0;

        let walker = if recursive {
            WalkDir::new(&resolved_path).min_depth(1)
        } else {
            WalkDir::new(&resolved_path).min_depth(1).max_depth(1)
        };

        for entry in walker {
            let entry = entry.map_err(|e| ToolError::internal(format!("Walk error: {e}")))?;
            let file_name = entry.file_name().to_string_lossy();

            if !include_hidden && file_name.starts_with('.') {
                continue;
            }

            let metadata = entry
                .metadata()
                .map_err(|e| ToolError::internal(format!("Metadata error: {e}")))?;

            let file_info = FileInfo {
                path: entry.path().to_string_lossy().to_string(),
                size: metadata.len(),
                modified: DateTime::from(
                    metadata
                        .modified()
                        .unwrap_or_else(|_| std::time::SystemTime::now()),
                ),
                is_directory: metadata.is_dir(),
            };

            total_size += file_info.size;
            if file_info.is_directory {
                dir_count += 1;
            } else {
                file_count += 1;
            }

            files.push(file_info);
        }

        self.log_operation(format!(
            "Listed directory: {path} (found {file_count} files, {dir_count} dirs)"
        ));

        Ok(DirectoryListing {
            path: resolved_path.to_string_lossy().to_string(),
            files,
            total_size,
            file_count,
            dir_count,
        })
    }

    #[universal_tool(
        description = "Read file contents",
        cli(name = "cat"),
        rest(method = "GET", path = "/files/content")
    )]
    async fn read_file(
        &self,
        #[universal_tool_param(description = "File path", source = "query")] path: String,
    ) -> Result<FileContent, ToolError> {
        let resolved_path = self.resolve_path(&path);

        if !resolved_path.exists() {
            return Err(ToolError::not_found(format!("File not found: {path}")));
        }

        if !resolved_path.is_file() {
            return Err(ToolError::invalid_input(format!("Not a file: {path}")));
        }

        let content = fs::read_to_string(&resolved_path)
            .map_err(|e| ToolError::internal(format!("Failed to read file: {e}")))?;

        self.log_operation(format!("Read file: {} ({} bytes)", path, content.len()));

        Ok(FileContent {
            path: resolved_path.to_string_lossy().to_string(),
            content,
            encoding: "utf-8".to_string(),
        })
    }

    #[universal_tool(
        description = "Create or write to a file",
        cli(name = "write"),
        rest(method = "POST", path = "/files")
    )]
    async fn create_file(
        &self,
        #[universal_tool_param(source = "body")] request: CreateFileRequest,
    ) -> Result<FileInfo, ToolError> {
        let resolved_path = self.resolve_path(&request.path);
        let overwrite = request.overwrite.unwrap_or(false);

        if resolved_path.exists() && !overwrite {
            return Err(ToolError::conflict(format!(
                "File already exists: {}",
                request.path
            )));
        }

        if let Some(parent) = resolved_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ToolError::internal(format!("Failed to create directories: {e}")))?;
        }

        fs::write(&resolved_path, &request.content)
            .map_err(|e| ToolError::internal(format!("Failed to write file: {e}")))?;

        let metadata = fs::metadata(&resolved_path)
            .map_err(|e| ToolError::internal(format!("Failed to get metadata: {e}")))?;

        self.log_operation(format!(
            "Created file: {} ({} bytes)",
            request.path,
            request.content.len()
        ));

        Ok(FileInfo {
            path: resolved_path.to_string_lossy().to_string(),
            size: metadata.len(),
            modified: DateTime::from(
                metadata
                    .modified()
                    .unwrap_or_else(|_| std::time::SystemTime::now()),
            ),
            is_directory: false,
        })
    }

    #[universal_tool(
        description = "Delete a file or directory",
        cli(name = "rm"),
        rest(method = "DELETE", path = "/files")
    )]
    async fn delete_file(
        &self,
        #[universal_tool_param(description = "Path to delete", source = "query")] path: String,
        #[universal_tool_param(
            description = "Recursive deletion for directories",
            source = "query"
        )]
        recursive: Option<bool>,
    ) -> Result<(), ToolError> {
        let resolved_path = self.resolve_path(&path);

        if !resolved_path.exists() {
            return Err(ToolError::not_found(format!("Path not found: {path}")));
        }

        if resolved_path.is_dir() {
            if recursive.unwrap_or(false) {
                fs::remove_dir_all(&resolved_path).map_err(|e| {
                    ToolError::internal(format!("Failed to remove directory: {e}"))
                })?;
            } else {
                fs::remove_dir(&resolved_path).map_err(|e| {
                    ToolError::internal(format!("Failed to remove directory: {e}"))
                })?;
            }
        } else {
            fs::remove_file(&resolved_path)
                .map_err(|e| ToolError::internal(format!("Failed to remove file: {e}")))?;
        }

        self.log_operation(format!("Deleted: {path}"));
        Ok(())
    }

    #[universal_tool(
        description = "Copy a file or directory",
        cli(name = "cp"),
        rest(method = "POST", path = "/files/copy")
    )]
    async fn copy_file(
        &self,
        #[universal_tool_param(source = "body")] request: CopyMoveRequest,
    ) -> Result<FileInfo, ToolError> {
        let source_path = self.resolve_path(&request.source);
        let dest_path = self.resolve_path(&request.destination);
        let overwrite = request.overwrite.unwrap_or(false);

        if !source_path.exists() {
            return Err(ToolError::not_found(format!(
                "Source not found: {}",
                request.source
            )));
        }

        if dest_path.exists() && !overwrite {
            return Err(ToolError::conflict(format!(
                "Destination already exists: {}",
                request.destination
            )));
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ToolError::internal(format!("Failed to create directories: {e}")))?;
        }

        if source_path.is_file() {
            fs::copy(&source_path, &dest_path)
                .map_err(|e| ToolError::internal(format!("Failed to copy file: {e}")))?;
        } else {
            return Err(ToolError::invalid_input(
                "Directory copying not implemented".to_string(),
            ));
        }

        let metadata = fs::metadata(&dest_path)
            .map_err(|e| ToolError::internal(format!("Failed to get metadata: {e}")))?;

        self.log_operation(format!(
            "Copied {} to {}",
            request.source, request.destination
        ));

        Ok(FileInfo {
            path: dest_path.to_string_lossy().to_string(),
            size: metadata.len(),
            modified: DateTime::from(
                metadata
                    .modified()
                    .unwrap_or_else(|_| std::time::SystemTime::now()),
            ),
            is_directory: metadata.is_dir(),
        })
    }

    #[universal_tool(
        description = "Search for text in files",
        cli(name = "grep"),
        rest(method = "GET", path = "/files/search")
    )]
    async fn search_files(
        &self,
        #[universal_tool_param(description = "Search query", source = "query")] query: String,
        #[universal_tool_param(description = "Directory to search in", source = "query")]
        path: String,
        #[universal_tool_param(description = "File extensions to search", source = "query")]
        extensions: Option<String>,
        #[universal_tool_param(description = "Case sensitive search", source = "query")]
        case_sensitive: Option<bool>,
    ) -> Result<SearchResults, ToolError> {
        let resolved_path = self.resolve_path(&path);
        let case_sensitive = case_sensitive.unwrap_or(true);
        let extensions: Vec<&str> = extensions
            .as_ref()
            .map(|e| e.split(',').collect())
            .unwrap_or_default();

        if !resolved_path.exists() {
            return Err(ToolError::not_found(format!("Path not found: {path}")));
        }

        let mut results = Vec::new();
        let mut files_searched = 0;

        let search_query = if case_sensitive {
            query.clone()
        } else {
            query.to_lowercase()
        };

        for entry in WalkDir::new(&resolved_path) {
            let entry = entry.map_err(|e| ToolError::internal(format!("Walk error: {e}")))?;

            if !entry.file_type().is_file() {
                continue;
            }

            let file_path = entry.path();

            if !extensions.is_empty() {
                if let Some(ext) = file_path.extension() {
                    if !extensions.contains(&ext.to_string_lossy().as_ref()) {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            files_searched += 1;

            let file = fs::File::open(file_path)
                .map_err(|e| ToolError::internal(format!("Failed to open file: {e}")))?;
            let reader = BufReader::new(file);

            for (line_number, line_result) in reader.lines().enumerate() {
                let line = line_result
                    .map_err(|e| ToolError::internal(format!("Failed to read line: {e}")))?;

                let search_line = if case_sensitive {
                    line.clone()
                } else {
                    line.to_lowercase()
                };

                if let Some(match_start) = search_line.find(&search_query) {
                    results.push(SearchResult {
                        path: file_path.to_string_lossy().to_string(),
                        line_number: line_number + 1,
                        line_content: line,
                        match_start,
                        match_end: match_start + search_query.len(),
                    });
                }
            }
        }

        let total_matches = results.len();

        self.log_operation(format!(
            "Searched for '{query}' in {path} ({files_searched} files, {total_matches} matches)"
        ));

        Ok(SearchResults {
            query,
            results,
            total_matches,
            files_searched,
        })
    }

    #[universal_tool(
        description = "Get file system statistics",
        cli(name = "stats"),
        rest(method = "GET", path = "/files/stats")
    )]
    async fn get_stats(
        &self,
        #[universal_tool_param(description = "Directory path", source = "query")] path: String,
    ) -> Result<FileStats, ToolError> {
        let resolved_path = self.resolve_path(&path);

        if !resolved_path.exists() {
            return Err(ToolError::not_found(format!("Path not found: {path}")));
        }

        let mut total_files = 0;
        let mut total_directories = 0;
        let mut total_size = 0u64;
        let mut largest_file: Option<FileInfo> = None;
        let mut most_recent_file: Option<FileInfo> = None;

        for entry in WalkDir::new(&resolved_path) {
            let entry = entry.map_err(|e| ToolError::internal(format!("Walk error: {e}")))?;
            let metadata = entry
                .metadata()
                .map_err(|e| ToolError::internal(format!("Metadata error: {e}")))?;

            if metadata.is_dir() {
                total_directories += 1;
            } else {
                total_files += 1;
                total_size += metadata.len();

                let file_info = FileInfo {
                    path: entry.path().to_string_lossy().to_string(),
                    size: metadata.len(),
                    modified: DateTime::from(
                        metadata
                            .modified()
                            .unwrap_or_else(|_| std::time::SystemTime::now()),
                    ),
                    is_directory: false,
                };

                if largest_file
                    .as_ref()
                    .is_none_or(|f| file_info.size > f.size)
                {
                    largest_file = Some(file_info.clone());
                }

                if most_recent_file
                    .as_ref()
                    .is_none_or(|f| file_info.modified > f.modified)
                {
                    most_recent_file = Some(file_info);
                }
            }
        }

        self.log_operation(format!("Generated stats for: {path}"));

        Ok(FileStats {
            total_files,
            total_directories,
            total_size,
            largest_file,
            most_recent_file,
        })
    }

    #[universal_tool(
        description = "View operation log",
        cli(name = "log"),
        rest(method = "GET", path = "/operations/log")
    )]
    async fn view_log(&self) -> Result<Vec<String>, ToolError> {
        let log = self
            .operation_log
            .lock()
            .map_err(|_| ToolError::internal("Failed to access log"))?;
        Ok(log.clone())
    }
}

#[derive(Parser)]
#[command(
    author,
    version,
    about = "File Manager - CLI, REST API, and MCP server"
)]
struct Args {
    /// Run as REST API server
    #[arg(long)]
    rest: bool,

    /// Run as MCP server
    #[arg(long)]
    mcp: bool,

    /// Port for REST API (default: 3000)
    #[arg(long, default_value = "3000")]
    port: u16,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,

    /// CLI subcommand and arguments
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

async fn run_mcp_server(file_manager: FileManager) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting MCP server on stdio");

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    // Send initial response
    let init_response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": "file-manager",
                "version": "1.0.0"
            },
            "capabilities": {
                "tools": true
            }
        }
    });

    writeln!(stdout, "{}", serde_json::to_string(&init_response)?)?;
    stdout.flush()?;

    // Process incoming requests
    let reader = BufReader::new(stdin);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(request) => {
                let method = request["method"].as_str().unwrap_or("");
                let params = &request["params"];
                let id = request["id"].clone();

                let response = match method {
                    "tools/list" => {
                        // Return list of available tools
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "tools": [
                                    {
                                        "name": "list_files",
                                        "description": "List files in a directory",
                                        "inputSchema": {
                                            "type": "object",
                                            "properties": {
                                                "path": { "type": "string", "description": "Directory path" },
                                                "include_hidden": { "type": "boolean", "description": "Include hidden files" },
                                                "recursive": { "type": "boolean", "description": "Recursive listing" }
                                            },
                                            "required": ["path"]
                                        }
                                    },
                                    {
                                        "name": "read_file",
                                        "description": "Read file contents",
                                        "inputSchema": {
                                            "type": "object",
                                            "properties": {
                                                "path": { "type": "string", "description": "File path" }
                                            },
                                            "required": ["path"]
                                        }
                                    },
                                    {
                                        "name": "search_files",
                                        "description": "Search for text in files",
                                        "inputSchema": {
                                            "type": "object",
                                            "properties": {
                                                "query": { "type": "string", "description": "Search query" },
                                                "path": { "type": "string", "description": "Directory to search in" },
                                                "extensions": { "type": "string", "description": "File extensions to search" },
                                                "case_sensitive": { "type": "boolean", "description": "Case sensitive search" }
                                            },
                                            "required": ["query", "path"]
                                        }
                                    }
                                ]
                            }
                        })
                    }
                    "tools/call" => {
                        // Handle tool calls via UTF's MCP handler
                        let tool_name = params["name"].as_str().unwrap_or("");
                        let tool_params = &params["arguments"];

                        match file_manager
                            .handle_mcp_call(tool_name, tool_params.clone())
                            .await
                        {
                            Ok(result) => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": result
                                })
                            }
                            Err(e) => {
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": {
                                        "code": -32603,
                                        "message": e.to_string()
                                    }
                                })
                            }
                        }
                    }
                    _ => {
                        // Unknown method
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32601,
                                "message": "Method not found"
                            }
                        })
                    }
                };

                writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
                stdout.flush()?;
            }
            Err(e) => {
                error!("Failed to parse request: {}", e);
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(format!("{}={}", env!("CARGO_PKG_NAME"), log_level))
        .with_target(false)
        .init();

    let file_manager = FileManager::new();

    if args.rest {
        // Run as REST API
        info!("Starting REST API server on port {}", args.port);

        let file_manager_arc = Arc::new(file_manager);
        let app = FileManager::create_rest_router(file_manager_arc)
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http());

        let addr = format!("0.0.0.0:{}", args.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        info!("File Manager REST API listening on: http://{}", addr);
        info!(
            "API endpoints available at: http://localhost:{}/api/v1",
            args.port
        );

        axum::serve(listener, app).await?;
    } else if args.mcp {
        // Run as MCP server
        run_mcp_server(file_manager).await?;
    } else {
        // Run as CLI
        let cli_args: Vec<String> = if args.args.is_empty() {
            std::env::args().collect()
        } else {
            let mut cli_args = vec![env!("CARGO_PKG_NAME").to_string()];
            cli_args.extend(args.args);
            cli_args
        };

        let matches = file_manager
            .create_cli_command()
            .try_get_matches_from(cli_args)
            .unwrap_or_else(|e| e.exit());

        if let Err(e) = file_manager.execute_cli(matches).await {
            error!("Error: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
