use anyhow::Context;
use anyhow::anyhow;
use std::path::Path;
use std::path::PathBuf;

pub const PINNED_OPENCODE_VERSION: &str = "1.3.3";
pub const OPENCODE_BINARY_ENV: &str = "OPENCODE_BINARY";
/// Environment variable for extra arguments between binary and `serve` command.
///
/// Useful for launchers like `bunx` where the full command is:
/// `bunx --yes opencode-ai@1.3.3 serve --hostname ... --port ...`
///
/// Example: `OPENCODE_BINARY=bunx OPENCODE_BINARY_ARGS="--yes opencode-ai@1.3.3"`
///
/// The `--yes` flag makes bunx non-interactive (skips confirmation prompts).
pub const OPENCODE_BINARY_ARGS_ENV: &str = "OPENCODE_BINARY_ARGS";

/// Configuration for launching the `OpenCode` server.
///
/// Supports both direct binary invocation and launcher-based invocation:
/// - Direct: `binary = "/path/to/opencode"`, `launcher_args = []`
/// - Launcher: `binary = "bunx"`, `launcher_args = ["--yes", "opencode-ai@1.3.3"]`
#[derive(Debug, Clone)]
pub struct LauncherConfig {
    /// Path to the binary (or launcher binary like `bunx`).
    pub binary: String,
    /// Extra arguments inserted between the binary and `serve` command.
    pub launcher_args: Vec<String>,
}

pub fn normalize_version(raw: &str) -> &str {
    let trimmed = raw.trim();
    trimmed.strip_prefix('v').unwrap_or(trimmed)
}

pub fn validate_exact_version(actual: Option<&str>) -> anyhow::Result<()> {
    let Some(actual) = actual else {
        return Err(anyhow!(
            "OpenCode /global/health did not return a version; expected {PINNED_OPENCODE_VERSION}"
        ));
    };

    let normalized = normalize_version(actual);
    if normalized != PINNED_OPENCODE_VERSION {
        return Err(anyhow!(
            "OpenCode version mismatch: expected {PINNED_OPENCODE_VERSION} but got {actual}"
        ));
    }

    Ok(())
}

pub fn default_pinned_binary_path(base_dir: &Path) -> PathBuf {
    base_dir
        .join(".opencode")
        .join("bin")
        .join(format!("opencode-v{PINNED_OPENCODE_VERSION}"))
}

pub fn resolve_opencode_binary(base_dir: &Path) -> anyhow::Result<PathBuf> {
    if let Ok(value) = std::env::var(OPENCODE_BINARY_ENV) {
        let value = value.trim();
        if !value.is_empty() {
            let path = PathBuf::from(value);
            return path.canonicalize().with_context(|| {
                format!("OPENCODE_BINARY points to missing path: {}", path.display())
            });
        }
    }

    let candidate = default_pinned_binary_path(base_dir);
    if candidate.exists() {
        return candidate
            .canonicalize()
            .with_context(|| format!("Failed to canonicalize {}", candidate.display()));
    }

    Err(anyhow!(
        "No pinned OpenCode binary found.\n\
         Expected OpenCode v{ver}.\n\
         Set OPENCODE_BINARY to a v{ver} 'opencode' binary, or install it at:\n  {path}",
        ver = PINNED_OPENCODE_VERSION,
        path = candidate.display(),
    ))
}

/// Parse launcher args from `OPENCODE_BINARY_ARGS` environment variable.
///
/// Splits on whitespace. Returns empty Vec if unset or empty.
pub fn parse_launcher_args() -> Vec<String> {
    match std::env::var(OPENCODE_BINARY_ARGS_ENV) {
        Ok(value) => {
            let value = value.trim();
            if value.is_empty() {
                Vec::new()
            } else {
                value.split_whitespace().map(String::from).collect()
            }
        }
        Err(_) => Vec::new(),
    }
}

/// Resolve the full launcher configuration for starting `OpenCode`.
///
/// When `OPENCODE_BINARY_ARGS` is set, the binary is used as a launcher
/// (e.g., `bunx`) and is not canonicalized (it should be in PATH).
///
/// When `OPENCODE_BINARY_ARGS` is not set, falls back to resolving a direct
/// binary path via `resolve_opencode_binary`.
pub fn resolve_launcher_config(base_dir: &Path) -> anyhow::Result<LauncherConfig> {
    let launcher_args = parse_launcher_args();

    if !launcher_args.is_empty() {
        // Launcher mode: binary is expected to be in PATH (e.g., bunx, npx)
        // Don't canonicalize - it's not a file path, it's a command
        let binary = std::env::var(OPENCODE_BINARY_ENV)
            .map_or_else(|_| "opencode".to_string(), |v| v.trim().to_string());

        if binary.is_empty() {
            return Err(anyhow!(
                "OPENCODE_BINARY_ARGS is set but OPENCODE_BINARY is empty.\n\
                 When using launcher args, set OPENCODE_BINARY to the launcher command (e.g., 'bunx')."
            ));
        }

        tracing::debug!(
            binary = %binary,
            launcher_args = ?launcher_args,
            "using launcher mode for OpenCode"
        );

        return Ok(LauncherConfig {
            binary,
            launcher_args,
        });
    }

    // Direct binary mode: resolve and canonicalize the path
    let binary = resolve_opencode_binary(base_dir)?;
    Ok(LauncherConfig {
        binary: binary.to_string_lossy().to_string(),
        launcher_args: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::OnceLock;

    /// Mutex to serialize env var tests (env vars are process-global).
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn normalize_strips_v_prefix() {
        assert_eq!(normalize_version("v1.3.3"), "1.3.3");
        assert_eq!(normalize_version("1.3.3"), "1.3.3");
        assert_eq!(normalize_version("  v1.3.3 "), "1.3.3");
    }

    #[test]
    fn validate_exact_version_enforces_pinned() {
        validate_exact_version(Some("1.3.3")).unwrap();
        validate_exact_version(Some("v1.3.3")).unwrap();
        assert!(validate_exact_version(Some("1.3.4")).is_err());
        assert!(validate_exact_version(None).is_err());
    }

    #[test]
    fn default_pinned_binary_path_uses_repo_local_recipe() {
        let base = Path::new("/tmp/project");
        assert_eq!(
            default_pinned_binary_path(base),
            PathBuf::from("/tmp/project/.opencode/bin/opencode-v1.3.3")
        );
    }

    #[test]
    fn parse_launcher_args_empty_when_unset() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::remove_var(OPENCODE_BINARY_ARGS_ENV) };
        assert!(parse_launcher_args().is_empty());
    }

    #[test]
    fn parse_launcher_args_splits_on_whitespace() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::set_var(OPENCODE_BINARY_ARGS_ENV, "opencode-ai@1.3.3") };
        assert_eq!(parse_launcher_args(), vec!["opencode-ai@1.3.3"]);

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::set_var(OPENCODE_BINARY_ARGS_ENV, "--yes opencode-ai@1.3.3") };
        assert_eq!(parse_launcher_args(), vec!["--yes", "opencode-ai@1.3.3"]);

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::remove_var(OPENCODE_BINARY_ARGS_ENV) };
    }

    #[test]
    fn parse_launcher_args_empty_string_returns_empty() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::set_var(OPENCODE_BINARY_ARGS_ENV, "   ") };
        assert!(parse_launcher_args().is_empty());

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::remove_var(OPENCODE_BINARY_ARGS_ENV) };
    }

    #[test]
    fn resolve_launcher_config_launcher_mode() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe {
            std::env::set_var(OPENCODE_BINARY_ENV, "bunx");
            std::env::set_var(OPENCODE_BINARY_ARGS_ENV, "opencode-ai@1.3.3");
        }

        let base = Path::new("/tmp/project");
        let config = resolve_launcher_config(base).unwrap();

        assert_eq!(config.binary, "bunx");
        assert_eq!(config.launcher_args, vec!["opencode-ai@1.3.3"]);

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe {
            std::env::remove_var(OPENCODE_BINARY_ENV);
            std::env::remove_var(OPENCODE_BINARY_ARGS_ENV);
        }
    }

    #[test]
    fn resolve_launcher_config_errors_when_args_set_but_binary_empty() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe {
            std::env::set_var(OPENCODE_BINARY_ENV, "   ");
            std::env::set_var(OPENCODE_BINARY_ARGS_ENV, "opencode-ai@1.3.3");
        }

        let base = Path::new("/tmp/project");
        let result = resolve_launcher_config(base);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("OPENCODE_BINARY is empty")
        );

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe {
            std::env::remove_var(OPENCODE_BINARY_ENV);
            std::env::remove_var(OPENCODE_BINARY_ARGS_ENV);
        }
    }
}
