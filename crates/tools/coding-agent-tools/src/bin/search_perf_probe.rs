use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use clap::Parser;
use clap::ValueEnum;
use coding_agent_tools::glob;
use coding_agent_tools::grep;
use coding_agent_tools::paths;
use coding_agent_tools::types::OutputMode;
use coding_agent_tools::types::SortOrder;
use coding_agent_tools::walker;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(
    about = "Compare fixed vs legacy coding-agent search ignore handling",
    long_about = "Compare fixed vs legacy coding-agent search ignore handling. With no arguments, this targets ~/repos/gw/monorepo when it exists, uses the gw-shopify preset, compares both modes, and times out each case after 10 seconds."
)]
struct Args {
    #[arg(long, env = "AGENTIC_SEARCH_PROBE_ROOT")]
    root: Option<PathBuf>,

    #[arg(long, env = "AGENTIC_SEARCH_PROBE_MODE", value_enum, default_value_t = BenchMode::Both)]
    mode: BenchMode,

    #[arg(long, env = "AGENTIC_SEARCH_PROBE_PRESET", value_enum, default_value_t = Preset::GwShopify)]
    preset: Preset,

    #[arg(long, env = "AGENTIC_SEARCH_PROBE_ITERATIONS", default_value_t = 1)]
    iterations: usize,

    #[arg(long, env = "AGENTIC_SEARCH_PROBE_TIMEOUT_SEC", default_value_t = 10)]
    timeout_sec: u64,

    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    format: OutputFormat,

    #[arg(long)]
    output: Option<PathBuf>,

    #[arg(long, hide = true)]
    run_one_json: bool,

    #[arg(long, hide = true, value_enum)]
    case_kind: Option<CaseKind>,

    #[arg(long, hide = true)]
    case_name: Option<String>,

    #[arg(long, hide = true)]
    case_pattern: Option<String>,

    #[arg(long, hide = true, default_value_t = 1)]
    iteration: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum BenchMode {
    Fixed,
    Legacy,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Preset {
    Smoke,
    Generic,
    GwShopify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Table,
    Jsonl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CaseKind {
    Glob,
    Grep,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RunStatus {
    Ok,
    Error,
    Timeout,
}

#[derive(Debug, Clone)]
struct SearchCase {
    name: String,
    kind: CaseKind,
    pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunResult {
    case: String,
    kind: CaseKind,
    pattern: String,
    mode: BenchMode,
    iteration: usize,
    status: RunStatus,
    elapsed_ms: u128,
    matches: Option<usize>,
    warnings: usize,
    has_more: Option<bool>,
    error: Option<String>,
    root: String,
    legacy_ignore_recheck: bool,
    host: Option<String>,
    profile: String,
}

impl BenchMode {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Legacy => "legacy",
            Self::Both => "both",
        }
    }
}

impl CaseKind {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Glob => "glob",
            Self::Grep => "grep",
        }
    }
}

impl RunStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Timeout => "timeout",
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.run_one_json {
        let result = run_one_from_args(&args)?;
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }

    if args.iterations == 0 {
        return Err(anyhow!("--iterations must be greater than zero"));
    }

    let root = resolve_root(args.root.as_ref())?;
    let cases = preset_cases(args.preset);
    let results = run_parent(&args, &root, &cases)?;

    let output = match args.format {
        OutputFormat::Table => format_table(&results),
        OutputFormat::Jsonl => format_jsonl(&results)?,
    };

    if let Some(path) = args.output {
        fs::write(&path, output).with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        print!("{output}");
    }

    Ok(())
}

fn run_parent(args: &Args, root: &str, cases: &[SearchCase]) -> Result<Vec<RunResult>> {
    let exe = std::env::current_exe().context("failed to determine current executable")?;
    let modes = modes_for(args.mode);
    let timeout = Duration::from_secs(args.timeout_sec);
    let mut results = Vec::new();

    for case in cases {
        for iteration in 1..=args.iterations {
            for mode in &modes {
                results.push(run_child(&exe, root, case, *mode, iteration, timeout)?);
            }
        }
    }

    Ok(results)
}

fn run_child(
    exe: &PathBuf,
    root: &str,
    case: &SearchCase,
    mode: BenchMode,
    iteration: usize,
    timeout: Duration,
) -> Result<RunResult> {
    let mut command = Command::new(exe);
    command
        .arg("--run-one-json")
        .arg("--root")
        .arg(root)
        .arg("--mode")
        .arg(mode.as_arg())
        .arg("--case-kind")
        .arg(case.kind.as_arg())
        .arg("--case-name")
        .arg(&case.name)
        .arg("--case-pattern")
        .arg(&case.pattern)
        .arg("--iteration")
        .arg(iteration.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    match mode {
        BenchMode::Fixed => {
            command.env_remove(walker::LEGACY_IGNORE_RECHECK_ENV);
        }
        BenchMode::Legacy => {
            command.env(walker::LEGACY_IGNORE_RECHECK_ENV, "1");
        }
        BenchMode::Both => return Err(anyhow!("child mode cannot be both")),
    }

    let start = Instant::now();
    let mut child = command.spawn().context("failed to spawn probe child")?;

    loop {
        if child.try_wait()?.is_some() {
            let output = child.wait_with_output()?;
            return parse_child_output(&output, root, case, mode, iteration, start.elapsed());
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let output = child.wait_with_output()?;
            return Ok(timeout_result(
                root,
                case,
                mode,
                iteration,
                start.elapsed(),
                &output.stderr,
            ));
        }

        thread::sleep(Duration::from_millis(25));
    }
}

fn parse_child_output(
    output: &std::process::Output,
    root: &str,
    case: &SearchCase,
    mode: BenchMode,
    iteration: usize,
    elapsed: Duration,
) -> Result<RunResult> {
    if output.status.success() {
        let result: RunResult = serde_json::from_slice(&output.stdout).with_context(|| {
            format!(
                "failed to parse child JSON for {}; stdout={} stderr={}",
                case.name,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
        Ok(result)
    } else {
        Ok(error_result(
            root,
            case,
            mode,
            iteration,
            elapsed,
            Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        ))
    }
}

fn run_one_from_args(args: &Args) -> Result<RunResult> {
    let kind = args
        .case_kind
        .ok_or_else(|| anyhow!("--case-kind is required with --run-one-json"))?;
    let name = args
        .case_name
        .clone()
        .ok_or_else(|| anyhow!("--case-name is required with --run-one-json"))?;
    let pattern = args
        .case_pattern
        .clone()
        .ok_or_else(|| anyhow!("--case-pattern is required with --run-one-json"))?;
    let mode = match args.mode {
        BenchMode::Fixed | BenchMode::Legacy => args.mode,
        BenchMode::Both => return Err(anyhow!("--mode both is invalid with --run-one-json")),
    };
    let root = resolve_root(args.root.as_ref())?;
    let case = SearchCase {
        name,
        kind,
        pattern,
    };

    Ok(run_one(&root, &case, mode, args.iteration))
}

fn run_one(root: &str, case: &SearchCase, mode: BenchMode, iteration: usize) -> RunResult {
    let start = Instant::now();
    match case.kind {
        CaseKind::Glob => {
            let result = glob::run(glob::GlobConfig {
                root: root.to_string(),
                pattern: case.pattern.clone(),
                ignore_globs: Vec::new(),
                include_hidden: false,
                sort: SortOrder::Name,
                head_limit: 1000,
                offset: 0,
            });
            match result {
                Ok(output) => ok_result(
                    root,
                    case,
                    mode,
                    iteration,
                    start.elapsed(),
                    output.entries.len(),
                    output.warnings.len(),
                    output.has_more,
                ),
                Err(error) => error_result(
                    root,
                    case,
                    mode,
                    iteration,
                    start.elapsed(),
                    Some(error.to_string()),
                ),
            }
        }
        CaseKind::Grep => {
            let result = grep::run(grep::GrepConfig {
                root: root.to_string(),
                pattern: case.pattern.clone(),
                mode: OutputMode::Files,
                include_globs: Vec::new(),
                ignore_globs: Vec::new(),
                include_hidden: false,
                case_insensitive: false,
                multiline: false,
                line_numbers: true,
                context: None,
                context_before: None,
                context_after: None,
                include_binary: false,
                head_limit: 1000,
                offset: 0,
            });
            match result {
                Ok(output) => ok_result(
                    root,
                    case,
                    mode,
                    iteration,
                    start.elapsed(),
                    output.lines.len(),
                    output.warnings.len(),
                    output.has_more,
                ),
                Err(error) => error_result(
                    root,
                    case,
                    mode,
                    iteration,
                    start.elapsed(),
                    Some(error.to_string()),
                ),
            }
        }
    }
}

fn ok_result(
    root: &str,
    case: &SearchCase,
    mode: BenchMode,
    iteration: usize,
    elapsed: Duration,
    matches: usize,
    warnings: usize,
    has_more: bool,
) -> RunResult {
    RunResult {
        case: case.name.clone(),
        kind: case.kind,
        pattern: case.pattern.clone(),
        mode,
        iteration,
        status: RunStatus::Ok,
        elapsed_ms: elapsed.as_millis(),
        matches: Some(matches),
        warnings,
        has_more: Some(has_more),
        error: None,
        root: root.to_string(),
        legacy_ignore_recheck: walker::legacy_ignore_recheck_enabled(),
        host: host_name(),
        profile: profile_name(),
    }
}

fn error_result(
    root: &str,
    case: &SearchCase,
    mode: BenchMode,
    iteration: usize,
    elapsed: Duration,
    error: Option<String>,
) -> RunResult {
    RunResult {
        case: case.name.clone(),
        kind: case.kind,
        pattern: case.pattern.clone(),
        mode,
        iteration,
        status: RunStatus::Error,
        elapsed_ms: elapsed.as_millis(),
        matches: None,
        warnings: 0,
        has_more: None,
        error,
        root: root.to_string(),
        legacy_ignore_recheck: walker::legacy_ignore_recheck_enabled(),
        host: host_name(),
        profile: profile_name(),
    }
}

fn timeout_result(
    root: &str,
    case: &SearchCase,
    mode: BenchMode,
    iteration: usize,
    elapsed: Duration,
    stderr: &[u8],
) -> RunResult {
    RunResult {
        case: case.name.clone(),
        kind: case.kind,
        pattern: case.pattern.clone(),
        mode,
        iteration,
        status: RunStatus::Timeout,
        elapsed_ms: elapsed.as_millis(),
        matches: None,
        warnings: 0,
        has_more: None,
        error: Some(String::from_utf8_lossy(stderr).trim().to_string()),
        root: root.to_string(),
        legacy_ignore_recheck: matches!(mode, BenchMode::Legacy),
        host: host_name(),
        profile: profile_name(),
    }
}

fn modes_for(mode: BenchMode) -> Vec<BenchMode> {
    match mode {
        BenchMode::Fixed => vec![BenchMode::Fixed],
        BenchMode::Legacy => vec![BenchMode::Legacy],
        BenchMode::Both => vec![BenchMode::Legacy, BenchMode::Fixed],
    }
}

fn resolve_root(root: Option<&PathBuf>) -> Result<String> {
    if let Some(root) = root {
        return paths::to_abs_string(root.to_string_lossy().as_ref())
            .map_err(|error| anyhow!(error));
    }

    if let Some(home) = std::env::var_os("HOME") {
        let gw_monorepo = PathBuf::from(home).join("repos/gw/monorepo");
        if gw_monorepo.exists() {
            return paths::to_abs_string(gw_monorepo.to_string_lossy().as_ref())
                .map_err(|error| anyhow!(error));
        }
    }

    paths::to_abs_string(".").map_err(|error| anyhow!(error))
}

fn preset_cases(preset: Preset) -> Vec<SearchCase> {
    match preset {
        Preset::Smoke => vec![
            glob_case("**/Cargo.toml"),
            glob_case("crates/tools/coding-agent-tools/src/**/*.rs"),
            grep_case("ask_agent|AskAgent"),
            grep_case("stdout|stderr|parse.*output"),
        ],
        Preset::Generic => vec![
            glob_case("**/Cargo.toml"),
            glob_case("**/package.json"),
            glob_case("**/*.rs"),
            glob_case("**/*.ts"),
            grep_case("ask_agent|AskAgent"),
            grep_case("stdout|stderr|parse.*output"),
            grep_case("dispatch|Dispatch|handle_request|process_request"),
            grep_case("lifecycle|Lifecycle|timeout|cancel"),
        ],
        Preset::GwShopify => vec![
            glob_case("**/package.json"),
            glob_case("**/tsconfig*.json"),
            glob_case("**/*shopify*"),
            glob_case("**/*Shopify*"),
            glob_case("**/*search*"),
            glob_case("**/*product*"),
            glob_case("**/*graphql*"),
            glob_case("**/*.graphql"),
            grep_case("shopify|Shopify"),
            grep_case("search_products|searchProducts|productSearch"),
            grep_case("GraphQL|graphql|connection"),
            grep_case(r"products_limit|productsLimit|limit.*300|\b300\b"),
            grep_case("Catalog API|catalog.*api|catalogApi"),
            grep_case("Storefront|Admin API|Product"),
        ],
    }
}

fn glob_case(pattern: &str) -> SearchCase {
    SearchCase {
        name: format!("glob:{pattern}"),
        kind: CaseKind::Glob,
        pattern: pattern.to_string(),
    }
}

fn grep_case(pattern: &str) -> SearchCase {
    SearchCase {
        name: format!("grep:{pattern}"),
        kind: CaseKind::Grep,
        pattern: pattern.to_string(),
    }
}

fn format_table(results: &[RunResult]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<44} {:<4} {:>4} {:<7} {:<7} {:>10} {:>8} {:>8} {:>10}",
        "case", "kind", "iter", "mode", "status", "elapsed", "matches", "warn", "speedup"
    );
    let _ = writeln!(out, "{}", "-".repeat(111));
    for result in results {
        let matches = result
            .matches
            .map_or_else(|| "-".to_string(), |count| count.to_string());
        let _ = writeln!(
            out,
            "{:<44} {:<4} {:>4} {:<7} {:<7} {:>10} {:>8} {:>8} {:>10}",
            truncate(&result.case, 44),
            result.kind.as_arg(),
            result.iteration,
            result.mode.as_arg(),
            result.status.as_str(),
            format!("{}ms", result.elapsed_ms),
            matches,
            result.warnings,
            speedup_text(result, results)
        );
    }
    out
}

fn format_jsonl(results: &[RunResult]) -> Result<String> {
    let mut out = String::new();
    for result in results {
        let _ = writeln!(out, "{}", serde_json::to_string(result)?);
    }
    Ok(out)
}

fn speedup_text(result: &RunResult, results: &[RunResult]) -> String {
    if result.mode != BenchMode::Fixed || result.status != RunStatus::Ok || result.elapsed_ms == 0 {
        return String::new();
    }

    let Some(legacy) = results.iter().find(|candidate| {
        candidate.case == result.case
            && candidate.iteration == result.iteration
            && candidate.mode == BenchMode::Legacy
    }) else {
        return String::new();
    };

    let ratio = legacy.elapsed_ms as f64 / result.elapsed_ms as f64;
    match legacy.status {
        RunStatus::Ok => format!("{ratio:.1}x"),
        RunStatus::Timeout => format!(">{ratio:.1}x"),
        RunStatus::Error => String::new(),
    }
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    truncated.push('~');
    truncated
}

fn host_name() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            fs::read_to_string("/proc/sys/kernel/hostname")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn profile_name() -> String {
    if cfg!(debug_assertions) {
        "debug".to_string()
    } else {
        "release".to_string()
    }
}
