//! Token and secret resolution utilities.
//!
//! This module provides utilities for resolving API tokens from
//! environment variables and CLI tools.

use std::process::Command;
use thiserror::Error;

/// Errors that can occur during secret resolution.
#[derive(Debug, Error)]
pub enum SecretsError {
    /// Token was not found in any source
    #[error("Token not found in env or gh")]
    NotFound,
    /// External command failed
    #[error("gh command failed: {0}")]
    CommandFailed(String),
    /// I/O error
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Resolve a GitHub token from environment or `gh auth token`.
///
/// Resolution order:
/// 1. `GITHUB_TOKEN` environment variable
/// 2. `GH_TOKEN` environment variable
/// 3. `gh auth token` command output
///
/// # Errors
///
/// Returns `SecretsError::NotFound` if no token could be found.
/// Returns `SecretsError::CommandFailed` if gh exists but returns an error.
/// Returns `SecretsError::Io` if gh cannot be executed.
pub fn resolve_github_token() -> Result<String, SecretsError> {
    // Check GITHUB_TOKEN first
    if let Ok(v) = std::env::var("GITHUB_TOKEN")
        && !v.trim().is_empty()
    {
        return Ok(v.trim().to_string());
    }

    // Check GH_TOKEN
    if let Ok(v) = std::env::var("GH_TOKEN")
        && !v.trim().is_empty()
    {
        return Ok(v.trim().to_string());
    }

    // Try gh auth token
    let out = Command::new("gh").args(["auth", "token"]).output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                Err(SecretsError::NotFound)
            } else {
                Ok(s)
            }
        }
        Ok(o) => Err(SecretsError::CommandFailed(
            String::from_utf8_lossy(&o.stderr).to_string(),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // gh not installed, that's fine
            Err(SecretsError::NotFound)
        }
        Err(e) => Err(SecretsError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests are limited since we can't easily mock environment
    // variables or external commands. Full testing would require:
    // - Setting env vars (affects other tests)
    // - Mocking Command (complex)
    //
    // The current tests verify basic error types.

    #[test]
    fn secrets_error_display() {
        let e = SecretsError::NotFound;
        assert!(e.to_string().contains("not found"));

        let e = SecretsError::CommandFailed("bad things".into());
        assert!(e.to_string().contains("bad things"));
    }
}
