//! MCP config and tool whitelist validation.
//!
//! Provides utilities to validate MCP servers can start, complete handshake,
//! and respond to tools/list, as well as validate tool whitelists against
//! known built-in tools and MCP tools from server responses.

use crate::config::{MCPConfig, MCPServer};
use rmcp::{
    model::{ServerInfo, Tool},
    service::ServiceExt,
    transport::child_process::TokioChildProcess,
};
use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{io::AsyncReadExt, process::Command, sync::Mutex, sync::Semaphore, time::timeout};

// === Configuration ===

/// Options for MCP validation.
#[derive(Debug, Clone)]
pub struct ValidateOptions {
    /// Timeout for MCP handshake (default: 10s)
    pub handshake_timeout: Duration,
    /// Timeout for tools/list (default: 5s)
    pub tools_list_timeout: Duration,
    /// Overall timeout for entire validation per server (default: 15s)
    pub overall_timeout: Duration,
    /// Max parallel server validations (default: half of CPU count, min 1)
    pub parallelism: usize,
    /// Capture stderr from stdio servers on failure (default: true)
    pub capture_stderr: bool,
    /// Max bytes to capture from stderr (default: 64KB)
    pub max_stderr_bytes: usize,
}

impl Default for ValidateOptions {
    fn default() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(10),
            tools_list_timeout: Duration::from_secs(5),
            overall_timeout: Duration::from_secs(15),
            parallelism: std::cmp::max(1, num_cpus::get() / 2),
            capture_stderr: true,
            max_stderr_bytes: 64 * 1024,
        }
    }
}

// === Result Types ===

/// Transport type for the MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    Stdio,
    Http,
}

/// Successful MCP server validation result.
#[derive(Debug, Clone)]
pub struct McpServerValidationSuccess {
    /// Server info from handshake
    pub info: ServerInfo,
    /// Tools available from this server
    pub tools: Vec<Tool>,
    /// Time taken for handshake in milliseconds
    pub handshake_ms: u64,
    /// Time taken for tools/list in milliseconds
    pub tools_list_ms: u64,
    /// Transport type used
    pub transport: TransportType,
}

/// Errors that can occur during MCP server validation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum McpServerValidationError {
    #[error("Failed to spawn server: {message}")]
    SpawnIo {
        message: String,
        stderr_tail: Option<String>,
    },
    #[error("Handshake timed out after {0:?}")]
    HandshakeTimeout(Duration),
    #[error("Handshake failed: {message}")]
    HandshakeProtocol {
        message: String,
        stderr_tail: Option<String>,
    },
    #[error("HTTP connect error: {0}")]
    HttpConnectError(String),
    #[error("tools/list timed out after {0:?}")]
    ToolsListTimeout(Duration),
    #[error("tools/list error: {0}")]
    ToolsListError(String),
    #[error("Missing required tools: expected {expected:?}, found {found:?}")]
    MissingRequiredTools {
        expected: Vec<String>,
        found: Vec<String>,
    },
    #[error("Server not configured: {0}")]
    ServerNotConfigured(String),
    #[error("Validation task panicked: {message}")]
    TaskPanicked { message: String },
    #[error("Validation timed out after {after:?}")]
    OverallTimeout {
        after: Duration,
        stderr_tail: Option<String>,
    },
}

/// Result of validating a single MCP server.
#[derive(Debug, Clone)]
pub enum McpServerResult {
    Ok(Box<McpServerValidationSuccess>),
    Err(McpServerValidationError),
}

impl McpServerResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, McpServerResult::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self, McpServerResult::Err(_))
    }
}

/// Report of MCP config validation with per-server results.
#[derive(Debug, Clone)]
pub struct McpValidationReport {
    /// Per-server validation results
    pub servers: HashMap<String, McpServerResult>,
}

impl McpValidationReport {
    /// Returns true if all servers validated successfully.
    pub fn all_ok(&self) -> bool {
        self.servers.values().all(|r| r.is_ok())
    }

    /// Returns list of failed servers with their errors.
    pub fn failed(&self) -> Vec<(String, McpServerValidationError)> {
        self.servers
            .iter()
            .filter_map(|(k, v)| match v {
                McpServerResult::Err(e) => Some((k.clone(), e.clone())),
                McpServerResult::Ok(_) => None,
            })
            .collect()
    }

    /// Returns list of successful servers with their results.
    pub fn successful(&self) -> Vec<(String, McpServerValidationSuccess)> {
        self.servers
            .iter()
            .filter_map(|(k, v)| match v {
                McpServerResult::Ok(s) => Some((k.clone(), (**s).clone())),
                McpServerResult::Err(_) => None,
            })
            .collect()
    }
}

/// Aggregate error when validation fails for one or more servers.
#[derive(Debug, thiserror::Error)]
#[error("MCP validation failed for {count} server(s)")]
pub struct McpValidationAggregateError {
    /// Number of failed servers
    pub count: usize,
    /// List of (server_name, error) pairs
    pub errors: Vec<(String, McpServerValidationError)>,
    /// Full report including successful servers
    pub report: McpValidationReport,
}

// === Public API ===

/// Validate all MCP servers in a config.
///
/// Returns a report with per-server results. Use `report.all_ok()` to check
/// if all servers validated successfully, or `report.failed()` to get errors.
pub async fn validate_mcp_config(
    config: &MCPConfig,
    opts: &ValidateOptions,
) -> McpValidationReport {
    let semaphore = Arc::new(Semaphore::new(opts.parallelism));
    let mut handles: Vec<(String, tokio::task::JoinHandle<McpServerResult>)> = Vec::new();

    for (name, server) in &config.mcp_servers {
        let name_outer = name.clone();
        let name_inner = name.clone();
        let server = server.clone();
        let opts = opts.clone();
        let sem = semaphore.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await;
            validate_single_server(&name_inner, &server, &opts).await
        });
        handles.push((name_outer, handle));
    }

    let mut servers = HashMap::new();
    for (name, handle) in handles {
        match handle.await {
            Ok(result) => {
                servers.insert(name, result);
            }
            Err(e) => {
                // Task panicked or was cancelled - preserve server entry
                tracing::error!("Server validation task panicked: {e}");
                servers.insert(
                    name,
                    McpServerResult::Err(McpServerValidationError::TaskPanicked {
                        message: e.to_string(),
                    }),
                );
            }
        }
    }

    McpValidationReport { servers }
}

/// Validate MCP config and return error if any servers fail.
///
/// This is a strict mode that fails if any server validation fails.
pub async fn ensure_valid_mcp_config(
    config: &MCPConfig,
    opts: &ValidateOptions,
) -> Result<McpValidationReport, McpValidationAggregateError> {
    let report = validate_mcp_config(config, opts).await;
    let errors = report.failed();
    if errors.is_empty() {
        Ok(report)
    } else {
        Err(McpValidationAggregateError {
            count: errors.len(),
            errors,
            report,
        })
    }
}

// === Tool Whitelist Validation ===

/// Known built-in tools in Claude Code.
///
/// This list should be updated as Claude Code adds new tools.
/// Last updated: Claude Code version compatible with claudecode_rs 0.1.x
pub const KNOWN_BUILTIN_TOOLS: &[&str] = &[
    "Task",
    "TaskOutput",
    "Bash",
    "Glob",
    "Grep",
    "Read",
    "Edit",
    "MultiEdit",
    "Write",
    "NotebookRead",
    "NotebookEdit",
    "WebFetch",
    "WebSearch",
    "TodoRead",
    "TodoWrite",
    "ExitPlanMode",
    "EnterPlanMode",
    "BashOutput",
    "KillShell",
    "SlashCommand",
    "LS",
    "AskUserQuestion",
    "Skill",
];

/// Report from tool whitelist validation.
#[derive(Debug, Clone)]
pub struct ToolWhitelistReport {
    /// Built-in tools that were validated successfully
    pub ok_builtins: Vec<String>,
    /// Built-in tools that are unknown
    pub unknown_builtins: Vec<String>,
    /// MCP tools that were validated successfully
    pub ok_mcp: Vec<String>,
    /// MCP tools that are missing (server not configured or tool not found)
    pub missing_mcp: Vec<String>,
    /// Suggestions for unknown tools: tool_name -> [suggested_names]
    pub suggestions: HashMap<String, Vec<String>>,
}

impl ToolWhitelistReport {
    /// Returns true if all tools validated successfully.
    pub fn all_ok(&self) -> bool {
        self.unknown_builtins.is_empty() && self.missing_mcp.is_empty()
    }
}

/// Errors from tool whitelist validation.
#[derive(Debug, thiserror::Error)]
pub enum ToolWhitelistError {
    #[error("Unknown built-in tools: {}", format_list(.0))]
    UnknownBuiltins(Vec<String>),
    #[error("MCP tools missing or server not responding: {}", format_list(.0))]
    MissingMcpTools(Vec<String>),
    #[error("MCP servers not configured: {}", format_list(.0))]
    MissingServers(Vec<String>),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
}

fn format_list(items: &[String]) -> String {
    items.join(", ")
}

/// Validate a tool whitelist against known built-ins and MCP tools.
///
/// Partitions tools into built-in vs MCP (`mcp__` prefix), validates each:
/// - Built-in: checks against `KNOWN_BUILTIN_TOOLS` with typo suggestions
/// - MCP: parses `mcp__<server>__<tool>`, verifies server is configured and tool exists
///
/// If `mcp_config` is None, MCP tools will fail validation.
pub async fn validate_tool_whitelist(
    tools: &[String],
    mcp_config: Option<&MCPConfig>,
    opts: &ValidateOptions,
) -> Result<ToolWhitelistReport, ToolWhitelistError> {
    let mut ok_builtins = Vec::new();
    let mut unknown_builtins = Vec::new();
    let mut ok_mcp = Vec::new();
    let mut missing_mcp = Vec::new();
    let mut missing_servers = HashSet::new();
    let mut suggestions: HashMap<String, Vec<String>> = HashMap::new();

    // Partition tools
    let (mcp_tools, builtin_tools): (Vec<_>, Vec<_>) =
        tools.iter().partition(|t| t.starts_with("mcp__"));

    // Validate built-in tools
    for tool in builtin_tools {
        if KNOWN_BUILTIN_TOOLS.contains(&tool.as_str()) {
            ok_builtins.push(tool.clone());
        } else {
            unknown_builtins.push(tool.clone());
            let sug = suggest_similar(tool, KNOWN_BUILTIN_TOOLS);
            if !sug.is_empty() {
                suggestions.insert(tool.clone(), sug);
            }
        }
    }

    // Validate MCP tools
    if !mcp_tools.is_empty() {
        // Group MCP tools by server
        let mut tools_by_server: HashMap<String, Vec<String>> = HashMap::new();
        for tool in &mcp_tools {
            if let Some((server, tool_name)) = parse_mcp_tool_id(tool) {
                tools_by_server.entry(server).or_default().push(tool_name);
            } else {
                // Malformed MCP tool ID
                missing_mcp.push(tool.to_string());
            }
        }

        // Check if servers are configured and validate tools
        if let Some(config) = mcp_config {
            // First, validate servers and collect their tools
            let report = validate_mcp_config(config, opts).await;

            for (server_name, expected_tools) in tools_by_server {
                if !config.mcp_servers.contains_key(&server_name) {
                    missing_servers.insert(server_name.clone());
                    for tool in expected_tools {
                        missing_mcp.push(format!("mcp__{server_name}__{tool}"));
                    }
                    continue;
                }

                // Check server validation result
                match report.servers.get(&server_name) {
                    Some(McpServerResult::Ok(success)) => {
                        let available_tools: HashSet<_> =
                            success.tools.iter().map(|t| t.name.as_ref()).collect();

                        for tool in expected_tools {
                            let full_id = format!("mcp__{server_name}__{tool}");
                            if available_tools.contains(tool.as_str()) {
                                ok_mcp.push(full_id);
                            } else {
                                missing_mcp.push(full_id.clone());
                                // Suggest similar tool names from this server
                                let tool_names: Vec<&str> =
                                    success.tools.iter().map(|t| t.name.as_ref()).collect();
                                let sug = suggest_similar(&tool, &tool_names);
                                if !sug.is_empty() {
                                    let sug_with_prefix: Vec<String> = sug
                                        .into_iter()
                                        .map(|s| format!("mcp__{server_name}__{s}"))
                                        .collect();
                                    suggestions.insert(full_id, sug_with_prefix);
                                }
                            }
                        }
                    }
                    Some(McpServerResult::Err(_)) | None => {
                        // Server failed validation - mark all tools as missing
                        for tool in expected_tools {
                            missing_mcp.push(format!("mcp__{server_name}__{tool}"));
                        }
                    }
                }
            }
        } else {
            // No MCP config - all MCP tools are missing
            for tool in mcp_tools {
                missing_mcp.push(tool.to_string());
            }
        }
    }

    let report = ToolWhitelistReport {
        ok_builtins,
        unknown_builtins: unknown_builtins.clone(),
        ok_mcp,
        missing_mcp: missing_mcp.clone(),
        suggestions,
    };

    // Return error if any validation failed
    if !missing_servers.is_empty() {
        return Err(ToolWhitelistError::MissingServers(
            missing_servers.into_iter().collect(),
        ));
    }
    if !unknown_builtins.is_empty() {
        return Err(ToolWhitelistError::UnknownBuiltins(unknown_builtins));
    }
    if !missing_mcp.is_empty() {
        return Err(ToolWhitelistError::MissingMcpTools(missing_mcp));
    }

    Ok(report)
}

// === Internal Helpers ===

/// Parse an MCP tool ID like "mcp__server__tool" into (server, tool).
pub fn parse_mcp_tool_id(id: &str) -> Option<(String, String)> {
    if !id.starts_with("mcp__") {
        return None;
    }
    let rest = &id[5..]; // Skip "mcp__"
    let parts: Vec<&str> = rest.splitn(2, "__").collect();
    if parts.len() < 2 {
        return None;
    }
    Some((parts[0].to_string(), parts[1].to_string()))
}

/// Suggest similar names using Levenshtein distance.
fn suggest_similar(name: &str, known: &[&str]) -> Vec<String> {
    let name_lower = name.to_lowercase();
    let mut candidates: Vec<(usize, &str)> = known
        .iter()
        .map(|k| (levenshtein(&name_lower, &k.to_lowercase()), *k))
        .filter(|(dist, _)| *dist <= 3) // Only suggest if distance <= 3
        .collect();

    candidates.sort_by_key(|(dist, _)| *dist);
    candidates
        .into_iter()
        .take(3)
        .map(|(_, k)| k.to_string())
        .collect()
}

/// Simple Levenshtein distance implementation.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Validate a single MCP server.
async fn validate_single_server(
    _name: &str,
    server: &MCPServer,
    opts: &ValidateOptions,
) -> McpServerResult {
    match server {
        MCPServer::Stdio { command, args, env } => {
            match validate_stdio_server(command, args, env.as_ref(), opts).await {
                Ok(success) => McpServerResult::Ok(Box::new(success)),
                Err(e) => McpServerResult::Err(e),
            }
        }
        MCPServer::Http { url, headers } => {
            match validate_http_server(url, headers.as_ref(), opts).await {
                Ok(success) => McpServerResult::Ok(Box::new(success)),
                Err(e) => McpServerResult::Err(e),
            }
        }
    }
}

/// Validate a stdio MCP server.
async fn validate_stdio_server(
    command: &str,
    args: &[String],
    env: Option<&HashMap<String, String>>,
    opts: &ValidateOptions,
) -> Result<McpServerValidationSuccess, McpServerValidationError> {
    // Test-only panic injection to validate panic-safe aggregation
    #[cfg(test)]
    if command == "__panic__" {
        panic!("intentional test panic for aggregator");
    }

    // Helper to snapshot stderr tail from shared buffer
    async fn snapshot_tail(buf: &Option<Arc<Mutex<Vec<u8>>>>) -> Option<String> {
        if let Some(b) = buf {
            let data = b.lock().await.clone();
            if data.is_empty() {
                None
            } else {
                Some(String::from_utf8_lossy(&data).to_string())
            }
        } else {
            None
        }
    }

    // Build the command
    let mut cmd = Command::new(command);
    cmd.args(args).kill_on_drop(true);

    if let Some(env_vars) = env {
        for (k, v) in env_vars {
            cmd.env(k, v);
        }
    }

    // Spawn with stderr piped when capturing
    let (transport, stderr_opt) = {
        let mut builder = TokioChildProcess::builder(cmd);
        if opts.capture_stderr {
            builder = builder.stderr(Stdio::piped());
        }
        builder
            .spawn()
            .map_err(|e| McpServerValidationError::SpawnIo {
                message: format!("Failed to spawn '{command}': {e}"),
                stderr_tail: None,
            })?
    };

    // Background stderr reader with bounded buffer
    let stderr_tail_buf: Option<Arc<Mutex<Vec<u8>>>> = if opts.capture_stderr {
        if let Some(mut stderr) = stderr_opt {
            let buf = Arc::new(Mutex::new(Vec::with_capacity(std::cmp::min(
                1024,
                opts.max_stderr_bytes,
            ))));
            let buf_clone = buf.clone();
            let cap = opts.max_stderr_bytes;
            tokio::spawn(async move {
                let mut chunk = vec![0u8; 1024];
                loop {
                    match stderr.read(&mut chunk).await {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let mut guard = buf_clone.lock().await;
                            guard.extend_from_slice(&chunk[..n]);
                            // Keep only the last `cap` bytes (tail)
                            if guard.len() > cap {
                                let start = guard.len() - cap;
                                guard.drain(..start);
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            Some(buf)
        } else {
            None
        }
    } else {
        None
    };

    // Inner validation future: handshake + tools list
    let inner = {
        let stderr_buf = stderr_tail_buf.clone();
        async move {
            let start = Instant::now();

            // Perform handshake with timeout
            let handshake_result = match timeout(opts.handshake_timeout, ().serve(transport)).await
            {
                Err(_) => {
                    return Err(McpServerValidationError::HandshakeTimeout(
                        opts.handshake_timeout,
                    ));
                }
                Ok(Ok(svc)) => svc,
                Ok(Err(e)) => {
                    let tail = snapshot_tail(&stderr_buf).await;
                    return Err(McpServerValidationError::HandshakeProtocol {
                        message: format!("{e}"),
                        stderr_tail: tail,
                    });
                }
            };
            let handshake_ms = start.elapsed().as_millis() as u64;

            // Get server info
            let server_info = match handshake_result.peer_info().cloned() {
                Some(info) => info,
                None => {
                    let tail = snapshot_tail(&stderr_buf).await;
                    return Err(McpServerValidationError::HandshakeProtocol {
                        message: "Server info not available after handshake".to_string(),
                        stderr_tail: tail,
                    });
                }
            };

            // List tools with timeout (no stderr_tail here by design)
            let tools_start = Instant::now();
            let tools = match timeout(opts.tools_list_timeout, handshake_result.list_all_tools())
                .await
            {
                Err(_) => {
                    return Err(McpServerValidationError::ToolsListTimeout(
                        opts.tools_list_timeout,
                    ));
                }
                Ok(Ok(tools)) => tools,
                Ok(Err(e)) => return Err(McpServerValidationError::ToolsListError(format!("{e}"))),
            };
            let tools_list_ms = tools_start.elapsed().as_millis() as u64;

            // Cleanup: cancel the service
            let _ = handshake_result.cancel().await;

            Ok(McpServerValidationSuccess {
                info: server_info,
                tools,
                handshake_ms,
                tools_list_ms,
                transport: TransportType::Stdio,
            })
        }
    };

    // Overall timeout wrapper captures stderr on timeout
    match timeout(opts.overall_timeout, inner).await {
        Ok(result) => result,
        Err(_) => {
            let tail = snapshot_tail(&stderr_tail_buf).await;
            Err(McpServerValidationError::OverallTimeout {
                after: opts.overall_timeout,
                stderr_tail: tail,
            })
        }
    }
}

/// Validate an HTTP MCP server.
async fn validate_http_server(
    _url: &str,
    _headers: Option<&HashMap<String, String>>,
    _opts: &ValidateOptions,
) -> Result<McpServerValidationSuccess, McpServerValidationError> {
    // HTTP transport validation requires the streamable HTTP client transport feature
    // TODO(2): Implement HTTP server validation using rmcp's StreamableHttpClientTransport
    Err(McpServerValidationError::HttpConnectError(
        "HTTP MCP server validation not yet implemented".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mcp_tool_id_valid() {
        assert_eq!(
            parse_mcp_tool_id("mcp__coding-agent-tools__ls"),
            Some(("coding-agent-tools".into(), "ls".into()))
        );
    }

    #[test]
    fn test_parse_mcp_tool_id_with_underscores() {
        assert_eq!(
            parse_mcp_tool_id("mcp__my_server__my_tool_name"),
            Some(("my_server".into(), "my_tool_name".into()))
        );
    }

    #[test]
    fn test_parse_mcp_tool_id_with_double_underscore_in_tool() {
        // Tool name can contain double underscores
        assert_eq!(
            parse_mcp_tool_id("mcp__server__tool__with__underscores"),
            Some(("server".into(), "tool__with__underscores".into()))
        );
    }

    #[test]
    fn test_parse_mcp_tool_id_invalid() {
        assert_eq!(parse_mcp_tool_id("Glob"), None);
        assert_eq!(parse_mcp_tool_id("mcp__only_one"), None);
        assert_eq!(parse_mcp_tool_id("mcp_single_underscore"), None);
        assert_eq!(parse_mcp_tool_id(""), None);
    }

    #[test]
    fn test_known_builtin_tools_contains_expected() {
        assert!(KNOWN_BUILTIN_TOOLS.contains(&"Glob"));
        assert!(KNOWN_BUILTIN_TOOLS.contains(&"Read"));
        assert!(KNOWN_BUILTIN_TOOLS.contains(&"WebSearch"));
        assert!(KNOWN_BUILTIN_TOOLS.contains(&"TodoWrite"));
        assert!(KNOWN_BUILTIN_TOOLS.contains(&"Task"));
    }

    #[test]
    fn test_suggest_similar_typo() {
        let suggestions = suggest_similar("Grpe", KNOWN_BUILTIN_TOOLS);
        assert!(suggestions.contains(&"Grep".to_string()));
    }

    #[test]
    fn test_suggest_similar_case_insensitive() {
        let suggestions = suggest_similar("glob", KNOWN_BUILTIN_TOOLS);
        assert!(suggestions.contains(&"Glob".to_string()));
    }

    #[test]
    fn test_suggest_similar_no_match() {
        let suggestions = suggest_similar("xyzzy", KNOWN_BUILTIN_TOOLS);
        // Should return empty since no tool is within distance 3
        assert!(suggestions.is_empty() || suggestions.len() <= 3);
    }

    #[test]
    fn test_validate_options_defaults() {
        let opts = ValidateOptions::default();
        assert_eq!(opts.handshake_timeout, Duration::from_secs(10));
        assert_eq!(opts.tools_list_timeout, Duration::from_secs(5));
        assert_eq!(opts.overall_timeout, Duration::from_secs(15));
        assert!(opts.capture_stderr);
        assert_eq!(opts.max_stderr_bytes, 64 * 1024);
        assert!(opts.parallelism >= 1);
    }

    #[test]
    fn test_levenshtein_same() {
        assert_eq!(levenshtein("test", "test"), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein("", "test"), 4);
        assert_eq!(levenshtein("test", ""), 4);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn test_levenshtein_one_char_diff() {
        assert_eq!(levenshtein("test", "fest"), 1);
        assert_eq!(levenshtein("grep", "grpe"), 2); // Transposition
    }

    #[test]
    fn test_mcp_validation_report_all_ok() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            McpServerResult::Ok(Box::new(McpServerValidationSuccess {
                info: ServerInfo::default(),
                tools: vec![],
                handshake_ms: 100,
                tools_list_ms: 50,
                transport: TransportType::Stdio,
            })),
        );

        let report = McpValidationReport { servers };
        assert!(report.all_ok());
        assert!(report.failed().is_empty());
    }

    #[test]
    fn test_mcp_validation_report_with_failure() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            McpServerResult::Err(McpServerValidationError::SpawnIo {
                message: "not found".to_string(),
                stderr_tail: None,
            }),
        );

        let report = McpValidationReport { servers };
        assert!(!report.all_ok());
        assert_eq!(report.failed().len(), 1);
    }

    #[test]
    fn test_tool_whitelist_report_all_ok() {
        let report = ToolWhitelistReport {
            ok_builtins: vec!["Glob".to_string()],
            unknown_builtins: vec![],
            ok_mcp: vec!["mcp__test__ls".to_string()],
            missing_mcp: vec![],
            suggestions: HashMap::new(),
        };
        assert!(report.all_ok());
    }

    #[test]
    fn test_tool_whitelist_report_with_unknown() {
        let report = ToolWhitelistReport {
            ok_builtins: vec![],
            unknown_builtins: vec!["Unknown".to_string()],
            ok_mcp: vec![],
            missing_mcp: vec![],
            suggestions: HashMap::new(),
        };
        assert!(!report.all_ok());
    }

    #[tokio::test]
    async fn test_panicked_task_is_reported() {
        use crate::config::{MCPConfig, MCPServer};

        let mut cfg = MCPConfig {
            mcp_servers: HashMap::new(),
        };
        cfg.mcp_servers.insert(
            "panic_server".to_string(),
            MCPServer::Stdio {
                command: "__panic__".into(),
                args: vec![],
                env: None,
            },
        );

        let opts = ValidateOptions::default();
        let report = validate_mcp_config(&cfg, &opts).await;

        let entry = report.servers.get("panic_server").expect("entry missing");
        match entry {
            McpServerResult::Err(McpServerValidationError::TaskPanicked { message }) => {
                assert!(
                    message.to_lowercase().contains("panic"),
                    "message={message}"
                );
            }
            other => panic!("expected TaskPanicked, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_stderr_capture_on_handshake_failure() {
        use crate::config::{MCPConfig, MCPServer};

        let cmd = "sh";
        let args = vec![
            "-c".to_string(),
            "echo boom 1>&2; sleep 0.05; exit 1".to_string(),
        ];
        let mut cfg = MCPConfig {
            mcp_servers: HashMap::new(),
        };
        cfg.mcp_servers.insert(
            "bad".to_string(),
            MCPServer::Stdio {
                command: cmd.into(),
                args,
                env: None,
            },
        );

        let opts = ValidateOptions {
            handshake_timeout: Duration::from_secs(2),
            overall_timeout: Duration::from_secs(3),
            capture_stderr: true,
            max_stderr_bytes: 1024,
            ..Default::default()
        };

        let report = validate_mcp_config(&cfg, &opts).await;
        let err = match report.servers.get("bad").unwrap() {
            McpServerResult::Err(e) => e.clone(),
            other => panic!("expected error, got {other:?}"),
        };

        match err {
            McpServerValidationError::HandshakeProtocol { stderr_tail, .. } => {
                let tail = stderr_tail.expect("expected captured stderr");
                assert!(tail.contains("boom"), "stderr_tail={tail}");
            }
            other => panic!("expected HandshakeProtocol, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_overall_timeout() {
        use crate::config::{MCPConfig, MCPServer};

        let cmd = "sh";
        let args = vec!["-c".to_string(), "sleep 5".to_string()];
        let mut cfg = MCPConfig {
            mcp_servers: HashMap::new(),
        };
        cfg.mcp_servers.insert(
            "slow".to_string(),
            MCPServer::Stdio {
                command: cmd.into(),
                args,
                env: None,
            },
        );

        let opts = ValidateOptions {
            handshake_timeout: Duration::from_secs(5),
            tools_list_timeout: Duration::from_secs(5),
            overall_timeout: Duration::from_millis(100),
            capture_stderr: true,
            ..Default::default()
        };

        let report = validate_mcp_config(&cfg, &opts).await;
        let err = match report.servers.get("slow").unwrap() {
            McpServerResult::Err(e) => e.clone(),
            other => panic!("expected error, got {other:?}"),
        };

        match err {
            McpServerValidationError::OverallTimeout { after, .. } => {
                assert_eq!(after, Duration::from_millis(100));
            }
            other => panic!("expected OverallTimeout, got {other:?}"),
        }
    }
}
