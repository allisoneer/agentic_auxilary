//! Unified MCP server for all agentic-tools.
//!
//! This binary exposes all 19+ tools from the various domain crates through a single
//! MCP stdio server, with optional allowlist filtering.

#[cfg(not(unix))]
compile_error!(
    "agentic-mcp only supports Unix-like platforms (Linux/macOS). Windows is not supported."
);

use agentic_config::loader::load_merged;
use agentic_tools_core::fmt::TextOptions;
use agentic_tools_mcp::OutputMode;
use agentic_tools_mcp::RegistryServer;
use agentic_tools_mcp::ServiceExt;
use agentic_tools_mcp::stdio;
use agentic_tools_registry::AgenticRuntimeConfig;
use agentic_tools_registry::AgenticTools;
use agentic_tools_registry::AgenticToolsConfig;
use clap::Parser;
use clap::ValueEnum;
use colored::Colorize;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "agentic-mcp")]
#[command(about = "Unified MCP server for all agentic-tools", version)]
struct Args {
    /// Comma-separated allowlist (case-insensitive). Example: `cli_ls,cli_grep,ask_reasoning_model`
    #[arg(long, value_name = "NAMES")]
    allow: Option<String>,

    /// JSON config file path for server settings (allowlist/output)
    #[arg(long = "server-config", value_name = "PATH")]
    server_config: Option<String>,

    /// List available tools and exit
    #[arg(long)]
    list_tools: bool,

    /// Output mode: text | structured (default: text)
    #[arg(long, value_parser = ["text", "structured"])]
    output: Option<String>,

    /// Suppress search reminder footer in grep/glob text output.
    #[arg(long)]
    suppress_search_reminder: bool,

    /// Runtime-only least-privilege profile for nested `ask_agent` servers.
    #[arg(long, value_enum)]
    nested_profile: Option<NestedProfile>,

    // Convenience flags for individual tool filtering
    // TODO(3): Probably don't need these convenience flags. They are kinda archaic for the old
    // agentic-tools setup. We likely can remove them after ensuring no one else uses them.
    /// Enable `cli_ls` tool
    #[arg(long)]
    cli_ls: bool,

    /// Enable `ask_agent` tool
    #[arg(long)]
    ask_agent: bool,

    /// Enable `cli_grep` tool
    #[arg(long)]
    cli_grep: bool,

    /// Enable `cli_glob` tool
    #[arg(long)]
    cli_glob: bool,

    /// Enable `cli_just_search` tool
    #[arg(long)]
    cli_just_search: bool,

    /// Enable `cli_just_execute` tool
    #[arg(long)]
    cli_just_execute: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum NestedProfile {
    Codebase,
    Thoughts,
    References,
    Web,
}

#[derive(Deserialize)]
struct FileConfig {
    allowlist: Option<HashSet<String>>,
    output: Option<String>,
}

fn parse_config(args: &Args) -> (AgenticToolsConfig, Option<String>) {
    // Parse server config if provided
    let mut allowlist: Option<HashSet<String>> = None;
    let mut file_output: Option<String> = None;
    if let Some(path) = args.server_config.as_deref() {
        match fs::read_to_string(path) {
            Ok(s) => {
                if let Ok(fc) = serde_json::from_str::<FileConfig>(&s) {
                    allowlist = fc.allowlist;
                    file_output = fc.output;
                } else {
                    eprintln!("Warning: Failed to parse config JSON; ignoring");
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to read config file: {e}; ignoring");
            }
        }
    }

    // Parse --allow if provided (wins over config file)
    if let Some(ref s) = args.allow {
        let set: HashSet<String> = s
            .split(',')
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect();
        if !set.is_empty() {
            allowlist = Some(set);
        }
    }

    // Merge convenience flags into allowlist
    let mut flag_set: HashSet<String> = HashSet::new();
    if args.cli_ls {
        flag_set.insert("cli_ls".to_string());
    }
    if args.ask_agent {
        flag_set.insert("ask_agent".to_string());
    }
    if args.cli_grep {
        flag_set.insert("cli_grep".to_string());
    }
    if args.cli_glob {
        flag_set.insert("cli_glob".to_string());
    }
    if args.cli_just_search {
        flag_set.insert("cli_just_search".to_string());
    }
    if args.cli_just_execute {
        flag_set.insert("cli_just_execute".to_string());
    }

    if !flag_set.is_empty() {
        allowlist.get_or_insert_with(HashSet::new).extend(flag_set);
    }

    (
        AgenticToolsConfig {
            allowlist,
            ..Default::default()
        },
        file_output,
    )
}

fn canonical_safe_root(path: &Path, label: &str) -> anyhow::Result<PathBuf> {
    let root = fs::canonicalize(path)
        .map_err(|error| anyhow::anyhow!("Failed to canonicalize {label} root: {error}"))?;
    if !root.is_dir() || root == Path::new("/") {
        anyhow::bail!("{label} root must be an existing non-root directory");
    }
    Ok(root)
}

fn references_root(cwd: &Path) -> anyhow::Result<PathBuf> {
    canonical_safe_root(
        &thoughts_tool::workspace::resolve_references_base_read_only(cwd)?,
        "References",
    )
}

fn allow_has(allowlist: Option<&HashSet<String>>, name: &str) -> bool {
    allowlist.is_some_and(|allow| allow.contains(name))
}

fn nested_profile_names(profile: NestedProfile) -> &'static [&'static str] {
    match profile {
        NestedProfile::Codebase => &[
            "cli_ls",
            "cli_grep",
            "cli_glob",
            "workspace_read",
            "workspace_todowrite",
        ],
        NestedProfile::Thoughts => &[
            "cli_ls",
            "cli_grep",
            "cli_glob",
            "thoughts_list_documents",
            "thoughts_read_document",
        ],
        NestedProfile::References => &[
            "cli_ls",
            "cli_grep",
            "cli_glob",
            "thoughts_list_references",
            "thoughts_read_reference",
            "workspace_todowrite",
        ],
        NestedProfile::Web => &["web_search", "web_fetch", "workspace_todowrite"],
    }
}

fn restrict_nested_allowlist(
    profile: NestedProfile,
    requested: Option<HashSet<String>>,
) -> HashSet<String> {
    let allowed = nested_profile_names(profile)
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut restricted = requested
        .filter(|requested| {
            !requested.is_empty() && requested.iter().all(|name| allowed.contains(name.as_str()))
        })
        .unwrap_or_default();
    if restricted.is_empty() {
        restricted.insert("__nested_profile_no_tools__".to_string());
    }
    restricted
}

fn exact_nested_cli_allowlist(profile: NestedProfile, raw: &str) -> Option<HashSet<String>> {
    let allowed = nested_profile_names(profile)
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut requested = HashSet::new();
    for name in raw.split(',') {
        if name.is_empty()
            || name.trim() != name
            || !allowed.contains(name)
            || !requested.insert(name.to_string())
        {
            return None;
        }
    }
    (!requested.is_empty()).then_some(requested)
}

fn nested_runtime(
    profile: NestedProfile,
    cwd: &Path,
    allowlist: Option<&HashSet<String>>,
) -> anyhow::Result<AgenticRuntimeConfig> {
    let worktree = thoughts_tool::git::utils::find_repo_root(cwd)?;
    let worktree = canonical_safe_root(&worktree, "worktree")?;
    let location_root = match profile {
        NestedProfile::Thoughts => Some(canonical_safe_root(
            &thoughts_tool::workspace::resolve_active_work_base_read_only(cwd)?,
            "Thoughts",
        )?),
        NestedProfile::References => Some(references_root(cwd)?),
        NestedProfile::Codebase | NestedProfile::Web => None,
    };
    Ok(nested_runtime_from_roots(
        profile,
        worktree,
        location_root,
        allowlist,
    ))
}

fn nested_runtime_from_roots(
    profile: NestedProfile,
    worktree: PathBuf,
    location_root: Option<PathBuf>,
    allowlist: Option<&HashSet<String>>,
) -> AgenticRuntimeConfig {
    let mut roots = vec![worktree];
    if let Some(root) = location_root
        && !roots.contains(&root)
    {
        roots.push(root);
    }

    AgenticRuntimeConfig {
        thoughts_read_document: matches!(profile, NestedProfile::Thoughts)
            && allow_has(allowlist, "thoughts_read_document"),
        thoughts_read_reference: matches!(profile, NestedProfile::References)
            && allow_has(allowlist, "thoughts_read_reference"),
        thoughts_read_only_nested: matches!(
            profile,
            NestedProfile::Thoughts | NestedProfile::References
        ),
        workspace_tools: Some(agentic_config::types::WorkspaceToolsConfig {
            workspace_read: allow_has(allowlist, "workspace_read")
                && matches!(profile, NestedProfile::Codebase),
            workspace_todowrite: allow_has(allowlist, "workspace_todowrite")
                && matches!(
                    profile,
                    NestedProfile::Codebase | NestedProfile::References | NestedProfile::Web
                ),
            workspace_edit: false,
            workspace_apply_patch: false,
        }),
        cli_roots: Some(roots),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Install the rustls CryptoProvider before any HTTP clients are created.
    // Required because Cargo's additive features cause both ring and aws-lc-rs
    // to be compiled in via transitive dependencies (async-openai, jsonwebtoken, etc.),
    // and rustls 0.23+ panics if it can't auto-select a single provider.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let args = Args::parse();

    // Load agentic.toml for tool-specific config (subagents, reasoning)
    let cwd = std::env::current_dir()?;
    let loaded = load_merged(&cwd)?;

    // Print config warnings
    for w in &loaded.warnings {
        eprintln!("{} {}", "WARN".yellow(), w);
    }

    // Parse server config (allowlist, output mode)
    let (mut reg_cfg, file_output) = parse_config(&args);

    // Attach tool config sections from agentic.toml
    reg_cfg.subagents = loaded.config.subagents.clone();
    reg_cfg.reasoning = loaded.config.reasoning.clone();
    reg_cfg.web_retrieval = loaded.config.web_retrieval.clone();
    reg_cfg.cli_tools = loaded.config.cli_tools.clone();
    reg_cfg.workspace_tools = loaded.config.workspace_tools.clone();
    reg_cfg.exa = loaded.config.services.exa.clone();
    reg_cfg.anthropic = loaded.config.services.anthropic.clone();
    reg_cfg.linear = loaded.config.services.linear.clone();
    reg_cfg.github = loaded.config.services.github.clone();
    reg_cfg.discord = loaded.config.services.discord.clone();
    reg_cfg.review = loaded.config.review.clone();
    reg_cfg.thoughts = loaded.config.thoughts.clone();
    if let Some(profile) = args.nested_profile {
        let requested = args.allow.as_deref().and_then(|raw| {
            let requested = exact_nested_cli_allowlist(profile, raw);
            if requested.is_none() {
                eprintln!("Rejected explicit nested --allow value; publishing zero tools");
            }
            requested
        });
        reg_cfg.allowlist = Some(restrict_nested_allowlist(profile, requested));
        reg_cfg.runtime = nested_runtime(profile, &cwd, reg_cfg.allowlist.as_ref())?;
    }

    let reg = AgenticTools::new(reg_cfg);

    if args.list_tools {
        let mut names = reg.list_names();
        names.sort();
        eprintln!("Available tools ({}):", names.len());
        for n in names {
            eprintln!("  - {n}");
        }
        return Ok(());
    }

    let output_mode = match (args.output.as_deref(), file_output.as_deref()) {
        (Some("structured"), _) | (None, Some("structured")) => OutputMode::Structured,
        _ => OutputMode::Text, // default
    };

    eprintln!(
        "Starting agentic-mcp ({} tools) with output mode: {:?}",
        reg.len(),
        output_mode
    );

    let server = RegistryServer::new(Arc::new(reg))
        .with_info("agentic-mcp", env!("CARGO_PKG_VERSION"))
        .with_output_mode(output_mode)
        .with_text_options(
            TextOptions::default().with_suppress_search_reminder(args.suppress_search_reminder),
        );
    let transport = stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_safe_root_rejects_files_and_filesystem_root() {
        let temp = tempfile::TempDir::new().expect("temporary directory");
        let file = temp.path().join("file");
        std::fs::write(&file, "x").expect("write fixture");
        assert!(canonical_safe_root(&file, "fixture").is_err());
        assert!(canonical_safe_root(Path::new("/"), "fixture").is_err());
        assert!(canonical_safe_root(&temp.path().join("missing"), "fixture").is_err());
    }

    #[test]
    fn web_runtime_never_enables_workspace_read_or_edit() {
        let allow = HashSet::from([
            "workspace_read".to_string(),
            "workspace_todowrite".to_string(),
            "workspace_edit".to_string(),
        ]);
        let runtime = nested_runtime(
            NestedProfile::Web,
            Path::new(env!("CARGO_MANIFEST_DIR")),
            Some(&allow),
        )
        .expect("web runtime");
        let workspace = runtime.workspace_tools.expect("workspace override");
        assert!(!workspace.workspace_read);
        assert!(workspace.workspace_todowrite);
        assert!(!workspace.workspace_edit);
        assert!(!workspace.workspace_apply_patch);
    }

    #[test]
    fn nested_profile_allowlists_match_location_boundaries() {
        let cases = [
            (
                NestedProfile::Codebase,
                [
                    "cli_ls",
                    "cli_grep",
                    "cli_glob",
                    "workspace_read",
                    "workspace_todowrite",
                ]
                .as_slice(),
            ),
            (
                NestedProfile::Thoughts,
                [
                    "cli_ls",
                    "cli_grep",
                    "cli_glob",
                    "thoughts_list_documents",
                    "thoughts_read_document",
                ]
                .as_slice(),
            ),
            (
                NestedProfile::References,
                [
                    "cli_ls",
                    "cli_grep",
                    "cli_glob",
                    "thoughts_list_references",
                    "thoughts_read_reference",
                    "workspace_todowrite",
                ]
                .as_slice(),
            ),
            (
                NestedProfile::Web,
                ["web_search", "web_fetch", "workspace_todowrite"].as_slice(),
            ),
        ];
        for (profile, expected) in cases {
            let expected = expected
                .iter()
                .map(|name| (*name).to_string())
                .collect::<HashSet<_>>();
            let actual = restrict_nested_allowlist(profile, Some(expected.clone()));
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn malformed_nested_allowlists_fail_closed() {
        for raw in [
            "CLI_LS",
            " cli_ls",
            "cli_ls ",
            "mcp__agentic-mcp__cli_ls",
            "prefix_cli_ls",
            "cli_ls_suffix",
            "unknown",
            "cli_ls,cli_ls",
            "",
        ] {
            assert!(
                exact_nested_cli_allowlist(NestedProfile::Codebase, raw).is_none(),
                "unexpectedly accepted {raw:?}"
            );
        }
    }

    #[test]
    fn nested_profile_without_allowlist_publishes_no_tools() {
        for profile in [
            NestedProfile::Codebase,
            NestedProfile::Thoughts,
            NestedProfile::References,
            NestedProfile::Web,
        ] {
            assert_eq!(
                restrict_nested_allowlist(profile, None),
                HashSet::from(["__nested_profile_no_tools__".to_string()])
            );
        }
    }

    #[test]
    fn nested_runtime_without_allowlist_enables_no_runtime_capabilities() {
        let worktree = tempfile::TempDir::new().expect("worktree fixture");
        let location = tempfile::TempDir::new().expect("location fixture");
        for profile in [
            NestedProfile::Codebase,
            NestedProfile::Thoughts,
            NestedProfile::References,
            NestedProfile::Web,
        ] {
            let runtime = nested_runtime_from_roots(
                profile,
                worktree.path().to_path_buf(),
                Some(location.path().to_path_buf()),
                None,
            );
            assert!(!runtime.thoughts_read_document);
            assert!(!runtime.thoughts_read_reference);
            let workspace = runtime.workspace_tools.expect("workspace policy");
            assert!(!workspace.workspace_read);
            assert!(!workspace.workspace_todowrite);
            assert!(!workspace.workspace_edit);
            assert!(!workspace.workspace_apply_patch);
        }
    }

    #[test]
    fn every_nested_profile_derives_exact_runtime_gates_and_roots() {
        let worktree = tempfile::TempDir::new().expect("worktree fixture");
        let location = tempfile::TempDir::new().expect("location fixture");
        let worktree = std::fs::canonicalize(worktree.path()).expect("canonical worktree");
        let location = std::fs::canonicalize(location.path()).expect("canonical location");
        let allow = HashSet::from([
            "workspace_read".to_string(),
            "workspace_todowrite".to_string(),
            "thoughts_read_document".to_string(),
            "thoughts_read_reference".to_string(),
        ]);

        for profile in [
            NestedProfile::Codebase,
            NestedProfile::Thoughts,
            NestedProfile::References,
            NestedProfile::Web,
        ] {
            let extra = matches!(profile, NestedProfile::Thoughts | NestedProfile::References)
                .then(|| location.clone());
            let runtime = nested_runtime_from_roots(profile, worktree.clone(), extra, Some(&allow));
            let workspace = runtime.workspace_tools.expect("workspace policy");
            assert!(!workspace.workspace_edit);
            assert!(!workspace.workspace_apply_patch);
            assert_eq!(
                workspace.workspace_read,
                matches!(profile, NestedProfile::Codebase)
            );
            assert_eq!(
                workspace.workspace_todowrite,
                matches!(
                    profile,
                    NestedProfile::Codebase | NestedProfile::References | NestedProfile::Web
                )
            );
            assert_eq!(
                runtime.thoughts_read_document,
                matches!(profile, NestedProfile::Thoughts)
            );
            assert_eq!(
                runtime.thoughts_read_reference,
                matches!(profile, NestedProfile::References)
            );
            assert_eq!(
                runtime.thoughts_read_only_nested,
                matches!(profile, NestedProfile::Thoughts | NestedProfile::References)
            );
            let roots = runtime.cli_roots.expect("CLI roots");
            assert_eq!(roots[0], worktree);
            assert!(roots.contains(&worktree));
            assert_eq!(
                roots.contains(&location),
                matches!(profile, NestedProfile::Thoughts | NestedProfile::References)
            );
        }
    }

    #[test]
    fn worktree_remains_first_root_and_following_mounts_are_deduplicated() {
        let base = tempfile::TempDir::new().expect("root fixture");
        let worktree = base.path().join("z-worktree");
        let location = base.path().join("a-location");
        std::fs::create_dir_all(&worktree).expect("worktree root");
        std::fs::create_dir_all(&location).expect("location root");
        let runtime = nested_runtime_from_roots(
            NestedProfile::Thoughts,
            worktree.clone(),
            Some(location.clone()),
            None,
        );
        assert_eq!(runtime.cli_roots, Some(vec![worktree.clone(), location]));

        let duplicate = nested_runtime_from_roots(
            NestedProfile::Thoughts,
            worktree.clone(),
            Some(worktree.clone()),
            None,
        );
        assert_eq!(duplicate.cli_roots, Some(vec![worktree]));
    }
}
