use anyhow::Context;
use anyhow::anyhow;
pub use opencode_rs::version::OPENCODE_BINARY_ARGS_ENV;
pub use opencode_rs::version::OPENCODE_BINARY_ENV;
pub use opencode_rs::version::PINNED_OPENCODE_VERSION;
pub use opencode_rs::version::validate_exact_version;
use std::path::Path;
use std::path::PathBuf;

/// Configuration for launching the `OpenCode` server.
///
/// Supports both direct binary invocation and launcher-based invocation:
/// - Direct: `binary = "/path/to/opencode"`, `launcher_args = []`
/// - Launcher: `binary = "bunx"`, `launcher_args = ["--yes", "opencode-ai@1.14.19"]`
#[derive(Debug, Clone)]
pub struct LauncherConfig {
    /// Path to the binary (or launcher binary like `bunx`).
    pub binary: String,
    /// Extra arguments inserted between the binary and `serve` command.
    pub launcher_args: Vec<String>,
}

#[cfg(test)]
pub fn normalize_version(raw: &str) -> &str {
    opencode_rs::version::normalize_version(raw)
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

    // Fall back to "opencode" in PATH
    tracing::warn!(
        "No pinned OpenCode binary found at {}; falling back to 'opencode' in PATH",
        candidate.display()
    );
    Ok(PathBuf::from("opencode"))
}

/// Parse launcher args from `OPENCODE_BINARY_ARGS` environment variable.
///
/// Splits on whitespace. Returns empty Vec if unset or empty.
///
/// Note: This uses simple whitespace splitting and does not support shell-style
/// quoting. Arguments containing spaces (e.g., `--message "hello world"`) will
/// be incorrectly split. This is acceptable for the documented use case
/// (`--yes opencode-ai@1.14.19`).
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
    use serial_test::serial;

    #[test]
    fn normalize_strips_v_prefix() {
        assert_eq!(normalize_version("v1.14.19"), "1.14.19");
        assert_eq!(normalize_version("1.14.19"), "1.14.19");
        assert_eq!(normalize_version("  v1.14.19 "), "1.14.19");
    }

    #[test]
    fn validate_exact_version_enforces_pinned() {
        validate_exact_version(Some(PINNED_OPENCODE_VERSION)).unwrap();
        validate_exact_version(Some(&format!("v{PINNED_OPENCODE_VERSION}"))).unwrap();
        assert!(validate_exact_version(Some("1.14.18")).is_err());
        assert!(validate_exact_version(None).is_err());
    }

    #[test]
    fn default_pinned_binary_path_uses_repo_local_recipe() {
        let base = Path::new("/tmp/project");
        assert_eq!(
            default_pinned_binary_path(base),
            PathBuf::from(format!(
                "/tmp/project/.opencode/bin/opencode-v{PINNED_OPENCODE_VERSION}"
            ))
        );
    }

    #[test]
    #[serial(env)]
    fn parse_launcher_args_empty_when_unset() {
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::remove_var(OPENCODE_BINARY_ARGS_ENV) };
        assert!(parse_launcher_args().is_empty());
    }

    #[test]
    #[serial(env)]
    fn parse_launcher_args_splits_on_whitespace() {
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_BINARY_ARGS_ENV, "opencode-ai@1.14.19") };
        assert_eq!(parse_launcher_args(), vec!["opencode-ai@1.14.19"]);

        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_BINARY_ARGS_ENV, "--yes opencode-ai@1.14.19") };
        assert_eq!(parse_launcher_args(), vec!["--yes", "opencode-ai@1.14.19"]);

        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::remove_var(OPENCODE_BINARY_ARGS_ENV) };
    }

    #[test]
    #[serial(env)]
    fn parse_launcher_args_empty_string_returns_empty() {
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_BINARY_ARGS_ENV, "   ") };
        assert!(parse_launcher_args().is_empty());

        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::remove_var(OPENCODE_BINARY_ARGS_ENV) };
    }

    #[test]
    #[serial(env)]
    fn resolve_launcher_config_launcher_mode() {
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe {
            std::env::set_var(OPENCODE_BINARY_ENV, "bunx");
            std::env::set_var(OPENCODE_BINARY_ARGS_ENV, "opencode-ai@1.14.19");
        }

        let base = Path::new("/tmp/project");
        let config = resolve_launcher_config(base).unwrap();

        assert_eq!(config.binary, "bunx");
        assert_eq!(config.launcher_args, vec!["opencode-ai@1.14.19"]);

        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe {
            std::env::remove_var(OPENCODE_BINARY_ENV);
            std::env::remove_var(OPENCODE_BINARY_ARGS_ENV);
        }
    }

    #[test]
    #[serial(env)]
    fn resolve_launcher_config_errors_when_args_set_but_binary_empty() {
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe {
            std::env::set_var(OPENCODE_BINARY_ENV, "   ");
            std::env::set_var(OPENCODE_BINARY_ARGS_ENV, "opencode-ai@1.14.19");
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

        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe {
            std::env::remove_var(OPENCODE_BINARY_ENV);
            std::env::remove_var(OPENCODE_BINARY_ARGS_ENV);
        }
    }
}
