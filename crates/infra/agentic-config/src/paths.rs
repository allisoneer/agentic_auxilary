//! XDG path resolution for configuration files.
//!
//! This module provides explicit XDG Base Directory handling to ensure
//! consistent paths across all Unix systems, including macOS (which doesn't
//! use `~/Library/Application Support` for XDG-style applications).

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Environment variable for test isolation of config paths.
const TEST_CONFIG_DIR_VAR: &str = "__AGENTIC_CONFIG_DIR_FOR_TESTS";

/// Read an env var, trim it, and return None if empty.
fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// XDG config base directory.
///
/// Precedence:
/// 1. `__AGENTIC_CONFIG_DIR_FOR_TESTS` (test hook)
/// 2. `XDG_CONFIG_HOME`
/// 3. `$HOME/.config`
///
/// This function explicitly implements XDG Base Directory behavior rather than
/// relying on `dirs::config_dir()`, which returns `~/Library/Application Support`
/// on macOS. Our documented path is `~/.config/agentic/agentic.json` across all
/// Unix systems.
pub fn xdg_config_home() -> Result<PathBuf> {
    if let Some(p) = env_path(TEST_CONFIG_DIR_VAR) {
        return Ok(p);
    }
    if let Some(p) = env_path("XDG_CONFIG_HOME") {
        return Ok(p);
    }
    let home = env_path("HOME")
        .or_else(dirs::home_dir)
        .context("Could not determine $HOME for XDG config path")?;
    Ok(home.join(".config"))
}

/// Get the agentic config directory (`~/.config/agentic`).
pub fn agentic_config_dir() -> Result<PathBuf> {
    Ok(xdg_config_home()?.join("agentic"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::EnvGuard;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_hook_overrides_xdg() {
        let _guard = EnvGuard::set(TEST_CONFIG_DIR_VAR, "/test/override");
        let result = xdg_config_home().unwrap();
        assert_eq!(result, PathBuf::from("/test/override"));
    }

    #[test]
    #[serial]
    fn test_xdg_config_home_honored() {
        let _g1 = EnvGuard::remove(TEST_CONFIG_DIR_VAR);
        let _guard = EnvGuard::set("XDG_CONFIG_HOME", "/custom/config");
        let result = xdg_config_home().unwrap();
        assert_eq!(result, PathBuf::from("/custom/config"));
    }

    #[test]
    #[serial]
    fn test_empty_xdg_falls_back_to_home() {
        let _g1 = EnvGuard::remove(TEST_CONFIG_DIR_VAR);
        let _g2 = EnvGuard::set("XDG_CONFIG_HOME", "");
        let _g3 = EnvGuard::set("HOME", "/home/test");
        let result = xdg_config_home().unwrap();
        assert_eq!(result, PathBuf::from("/home/test/.config"));
    }

    #[test]
    #[serial]
    fn test_agentic_config_dir() {
        let _guard = EnvGuard::set(TEST_CONFIG_DIR_VAR, "/test/base");
        let result = agentic_config_dir().unwrap();
        assert_eq!(result, PathBuf::from("/test/base/agentic"));
    }
}
