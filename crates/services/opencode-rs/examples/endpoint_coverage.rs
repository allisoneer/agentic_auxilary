//! Endpoint coverage tool for opencode-rs SDK.
//!
//! Reports the SDK endpoint inventory and any deterministic inventory issues.

use std::collections::BTreeMap;
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
    ("POST", "/session/{id}/unrevert"),
    ("POST", "/session/{id}/init"),
    ("GET", "/session/{id}/diff"),
    ("GET", "/session/{id}/todo"),
    ("PATCH", "/session/{id}"),
    ("GET", "/session/status"),
    ("GET", "/session/{id}/children"),
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
    // pty.rs
    ("GET", "/pty"),
    ("POST", "/pty"),
    ("GET", "/pty/{id}"),
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
    // v2 core groups
    ("GET", "/api/health"),
    ("GET", "/api/location"),
    ("GET", "/api/session"),
    ("POST", "/api/session"),
    ("GET", "/api/session/{id}"),
    ("POST", "/api/session/{id}/prompt"),
    ("POST", "/api/session/{id}/compact"),
    ("POST", "/api/session/{id}/wait"),
    ("GET", "/api/session/{id}/context"),
    ("GET", "/api/session/{id}/message"),
    ("GET", "/api/model"),
    ("GET", "/api/provider"),
    ("GET", "/api/provider/{id}"),
    ("GET", "/api/permission/request"),
    ("GET", "/api/session/{id}/permission"),
    ("POST", "/api/session/{id}/permission/{requestId}/reply"),
    ("GET", "/api/question/request"),
    ("GET", "/api/session/{id}/question"),
    ("POST", "/api/session/{id}/question/{requestId}/reply"),
    ("POST", "/api/session/{id}/question/{requestId}/reject"),
    // v2 optional additive groups
    ("GET", "/api/connector"),
    ("GET", "/api/connector/{id}"),
    ("POST", "/api/connector/{id}/connect/key"),
    ("POST", "/api/connector/{id}/connect/oauth"),
    ("GET", "/api/connector/oauth/{attemptId}"),
    ("POST", "/api/connector/oauth/{attemptId}/complete"),
    ("DELETE", "/api/connector/oauth/{attemptId}"),
    ("GET", "/api/fs/list"),
    ("GET", "/api/fs/find"),
    ("GET", "/api/reference"),
];

/// Endpoints intentionally not implemented in SDK.
const SKIP_ENDPOINTS: &[(&str, &str)] = &[
    // Global endpoints not yet implemented in the SDK
    ("GET", "/global/config"),
    ("PATCH", "/global/config"),
    ("POST", "/global/upgrade"),
    // TUI-specific endpoints
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
    // Explicitly deferred optional V2 coverage
    ("GET", "/api/fs/read/{path}"),
    ("GET", "/api/event"),
    ("GET", "/api/permission/saved"),
    ("DELETE", "/api/permission/saved/{id}"),
];

fn normalize_path(path: &str) -> String {
    let mut normalized = path.to_string();

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
    normalized = normalized.replace("{requestID}", "{requestId}");
    normalized = normalized.replace("{connectorID}", "{id}");
    normalized = normalized.replace("{attemptID}", "{attemptId}");
    normalized = normalized.replace('*', "{path}");

    normalized
}

fn duplicate_entries(entries: &[(&str, &str)]) -> Vec<String> {
    let mut counts = BTreeMap::new();
    for (method, path) in entries {
        let key = format!("{} {}", method, normalize_path(path));
        *counts.entry(key).or_insert(0usize) += 1;
    }

    counts
        .into_iter()
        .filter_map(|(key, count)| (count > 1).then_some(key))
        .collect()
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: endpoint_coverage [--json]");
        println!();
        println!("Options:");
        println!("  --json           Output results as JSON");
        println!("  --help, -h       Show this help message");
        return ExitCode::SUCCESS;
    }

    let json_output = args.iter().any(|a| a == "--json");

    let sdk_set: BTreeSet<(String, String)> = SDK_ENDPOINTS
        .iter()
        .map(|(m, p)| (m.to_string(), normalize_path(p)))
        .collect();

    let skip_set: BTreeSet<(String, String)> = SKIP_ENDPOINTS
        .iter()
        .map(|(m, p)| (m.to_string(), normalize_path(p)))
        .collect();

    let duplicate_sdk_endpoints = duplicate_entries(SDK_ENDPOINTS);
    let duplicate_skip_endpoints = duplicate_entries(SKIP_ENDPOINTS);
    let overlapping_endpoints: Vec<String> = sdk_set
        .intersection(&skip_set)
        .map(|(method, path)| format!("{method} {path}"))
        .collect();

    if json_output {
        let output = serde_json::json!({
            "comparison_mode": "inventory-only",
            "live_diff_performed": false,
            "sdk_endpoints": sdk_set.iter().map(|(m, p)| format!("{m} {p}")).collect::<Vec<_>>(),
            "skipped_endpoints": skip_set.iter().map(|(m, p)| format!("{m} {p}")).collect::<Vec<_>>(),
            "sdk_count": sdk_set.len(),
            "skipped_count": skip_set.len(),
            "duplicate_sdk_endpoints": duplicate_sdk_endpoints,
            "duplicate_skip_endpoints": duplicate_skip_endpoints,
            "overlapping_endpoints": overlapping_endpoints,
            "missing_endpoints": Vec::<String>::new(),
            "extra_endpoints": Vec::<String>::new(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("=== SDK Endpoint Coverage Report ===");
        println!();
        println!("Mode: inventory-only (no live OpenAPI diff performed)");
        println!();
        println!("SDK Endpoints ({})", sdk_set.len());
        for (method, path) in &sdk_set {
            println!("  {method} {path}");
        }
        println!();
        println!("Intentionally Skipped ({})", skip_set.len());
        for (method, path) in &skip_set {
            println!("  {method} {path}");
        }
        println!();

        if !duplicate_sdk_endpoints.is_empty() {
            println!("Duplicate SDK endpoints:");
            for endpoint in &duplicate_sdk_endpoints {
                println!("  {endpoint}");
            }
            println!();
        }
        if !duplicate_skip_endpoints.is_empty() {
            println!("Duplicate skipped endpoints:");
            for endpoint in &duplicate_skip_endpoints {
                println!("  {endpoint}");
            }
            println!();
        }
        if !overlapping_endpoints.is_empty() {
            println!("Overlapping SDK/skipped endpoints:");
            for endpoint in &overlapping_endpoints {
                println!("  {endpoint}");
            }
            println!();
        }

        println!("Live OpenAPI diff is not implemented in this upgrade slice.");
    }

    ExitCode::SUCCESS
}
