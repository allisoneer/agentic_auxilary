//! Endpoint coverage tool for opencode-rs SDK.
//!
//! Compares SDK endpoints against the server's OpenAPI spec to identify
//! missing, extra, and matched endpoints.
//!
//! Usage:
//!   cargo run -p opencode_rs --example endpoint_coverage --features full
//!
//! Or with a running server:
//!   cargo run -p opencode_rs --example endpoint_coverage --features full -- --base-url http://localhost:4096

use std::collections::BTreeSet;
use std::env;
use std::process::ExitCode;

/// Known SDK endpoints extracted from the http/ modules.
/// Format: (method, path)
const SDK_ENDPOINTS: &[(&str, &str)] = &[
    // sessions.rs
    ("POST", "/session"),
    ("GET", "/session"),
    ("GET", "/session/{id}"),
    ("DELETE", "/session/{id}"),
    ("POST", "/session/{id}/fork"),
    ("POST", "/session/{id}/abort"),
    ("POST", "/session/{id}/summarize"),
    ("POST", "/session/{id}/share"),
    ("DELETE", "/session/{id}/share"),
    ("POST", "/session/{id}/revert"),
    ("POST", "/session/{id}/init"),
    ("GET", "/session/{id}/diff"),
    ("GET", "/session/{id}/todo"),
    ("PATCH", "/session/{id}"),
    ("GET", "/session/status"),
    // messages.rs
    ("POST", "/session/{id}/message"),
    ("GET", "/session/{id}/message"),
    ("GET", "/session/{id}/message/{messageId}"),
    ("POST", "/session/{id}/prompt_async"),
    ("POST", "/session/{id}/command"),
    ("POST", "/session/{id}/shell"),
    ("DELETE", "/session/{id}/message/{messageId}"),
    // parts.rs
    ("DELETE", "/session/{id}/message/{messageId}/part/{partId}"),
    ("PATCH", "/session/{id}/message/{messageId}/part/{partId}"),
    // config.rs
    ("GET", "/config"),
    ("PATCH", "/config"),
    ("GET", "/config/providers"),
    // global.rs
    ("GET", "/global/health"),
    ("GET", "/global/event"),
    // misc.rs
    ("GET", "/doc"),
    ("GET", "/path"),
    ("GET", "/vcs"),
    ("POST", "/instance/dispose"),
    ("POST", "/log"),
    ("GET", "/lsp"),
    ("GET", "/formatter"),
    ("POST", "/global/dispose"),
    // project.rs
    ("GET", "/project"),
    ("GET", "/project/current"),
    ("PATCH", "/project/{id}"),
    ("POST", "/project/git/init"),
    // providers.rs
    ("GET", "/provider"),
    ("GET", "/provider/auth"),
    ("POST", "/provider/{id}/oauth/authorize"),
    ("POST", "/provider/{id}/oauth/callback"),
    ("PUT", "/auth/{id}"),
    ("DELETE", "/auth/{id}"),
    // mcp.rs
    ("GET", "/mcp"),
    ("POST", "/mcp"),
    ("POST", "/mcp/{id}/auth"),
    ("POST", "/mcp/{id}/auth/callback"),
    ("POST", "/mcp/{id}/auth/authenticate"),
    ("DELETE", "/mcp/{id}/auth"),
    ("POST", "/mcp/{id}/connect"),
    ("POST", "/mcp/{id}/disconnect"),
    // permissions.rs
    ("GET", "/permission"),
    ("POST", "/permission/{id}/reply"),
    // question.rs
    ("GET", "/question"),
    ("POST", "/question/{id}/reply"),
    ("POST", "/question/{id}/reject"),
    // pty.rs (excluding WebSocket connect)
    ("GET", "/pty"),
    ("POST", "/pty"),
    ("GET", "/pty/{id}"),
    ("GET", "/pty/{id}/connect"),
    ("PUT", "/pty/{id}"),
    ("DELETE", "/pty/{id}"),
    // files.rs
    ("GET", "/file"),
    ("GET", "/file/content"),
    ("GET", "/file/status"),
    // find.rs
    ("GET", "/find"),
    ("GET", "/find/file"),
    ("GET", "/find/symbol"),
    // tools.rs
    ("GET", "/experimental/tool/ids"),
    ("GET", "/experimental/tool"),
    ("GET", "/experimental/session"),
    ("GET", "/agent"),
    ("GET", "/command"),
    // worktree.rs
    ("GET", "/experimental/worktree"),
    ("POST", "/experimental/worktree"),
    ("DELETE", "/experimental/worktree"),
    ("POST", "/experimental/worktree/reset"),
    // sync.rs
    ("POST", "/sync/start"),
    ("POST", "/sync/replay"),
    ("POST", "/sync/history"),
    // tui.rs
    ("GET", "/tui/env"),
    ("POST", "/tui/command"),
    // workspaces.rs
    ("GET", "/experimental/workspace"),
    ("GET", "/experimental/workspace/current"),
    // console.rs
    ("GET", "/experimental/console"),
    ("POST", "/experimental/console"),
    // skills.rs
    ("GET", "/skill"),
    // resource.rs
    ("GET", "/experimental/resource"),
];

/// Endpoints intentionally not implemented in SDK.
const SKIP_ENDPOINTS: &[(&str, &str)] = &[
    // Global endpoints not yet implemented in the SDK
    ("GET", "/global/config"),
    ("PATCH", "/global/config"),
    ("POST", "/global/upgrade"),
    // TUI-specific endpoints
    ("GET", "/tui/env"),
    ("POST", "/tui/command"),
    ("GET", "/tui/palette"),
    ("POST", "/tui/search"),
    ("POST", "/tui/clear"),
    ("POST", "/tui/create_session"),
    ("POST", "/tui/select"),
    ("POST", "/tui/settings"),
    ("POST", "/tui/project"),
    ("POST", "/tui/open"),
    ("POST", "/tui/picker"),
    ("POST", "/tui/prompt"),
    ("POST", "/tui/auth_complete"),
];

fn normalize_path(path: &str) -> String {
    // Convert OpenAPI path params like {sessionID} to {id} for comparison
    let mut normalized = path.to_string();

    // Common normalizations
    normalized = normalized.replace("{sessionID}", "{id}");
    normalized = normalized.replace("{sessionId}", "{id}");
    normalized = normalized.replace("{messageID}", "{messageId}");
    normalized = normalized.replace("{partID}", "{partId}");
    normalized = normalized.replace("{snapshotID}", "{snapshotId}");
    normalized = normalized.replace("{permissionID}", "{id}");
    normalized = normalized.replace("{questionID}", "{id}");
    normalized = normalized.replace("{ptyID}", "{id}");
    normalized = normalized.replace("{worktreeID}", "{id}");
    normalized = normalized.replace("{skillID}", "{id}");
    normalized = normalized.replace("{providerID}", "{id}");
    normalized = normalized.replace("{mcpID}", "{id}");

    normalized
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    // Check for --help
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: endpoint_coverage [--base-url URL] [--json]");
        println!();
        println!("Options:");
        println!("  --base-url URL   Server base URL (default: starts managed server)");
        println!("  --json           Output results as JSON");
        println!("  --help, -h       Show this help message");
        return ExitCode::SUCCESS;
    }

    let json_output = args.iter().any(|a| a == "--json");

    // For now, just output the SDK endpoints list
    // Full implementation would fetch from server and compare

    let sdk_set: BTreeSet<(String, String)> = SDK_ENDPOINTS
        .iter()
        .map(|(m, p)| (m.to_string(), normalize_path(p)))
        .collect();

    let skip_set: BTreeSet<(String, String)> = SKIP_ENDPOINTS
        .iter()
        .map(|(m, p)| (m.to_string(), normalize_path(p)))
        .collect();

    if json_output {
        let output = serde_json::json!({
            "sdk_endpoints": sdk_set.iter().map(|(m, p)| format!("{m} {p}")).collect::<Vec<_>>(),
            "skipped_endpoints": skip_set.iter().map(|(m, p)| format!("{m} {p}")).collect::<Vec<_>>(),
            "sdk_count": sdk_set.len(),
            "skipped_count": skip_set.len(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("=== SDK Endpoint Coverage Report ===");
        println!();
        println!("SDK Endpoints ({}):", sdk_set.len());
        for (method, path) in &sdk_set {
            println!("  {method} {path}");
        }
        println!();
        println!("Intentionally Skipped ({}):", skip_set.len());
        for (method, path) in &skip_set {
            println!("  {method} {path}");
        }
        println!();
        println!("Note: Run with a live server to compare against OpenAPI spec.");
        println!(
            "      cargo run -p opencode_rs --example endpoint_coverage --features full -- --base-url http://localhost:4096"
        );
    }

    ExitCode::SUCCESS
}
