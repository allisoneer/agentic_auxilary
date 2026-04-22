//! Version pinning for opencode-rs SDK.
//!
//! Provides version constants and validation for ensuring SDK compatibility
//! with specific opencode server versions.

use crate::error::OpencodeError;
use crate::error::Result;

/// Pinned opencode server version for SDK compatibility testing.
pub const PINNED_OPENCODE_VERSION: &str = "1.14.19";

/// Environment variable for the opencode binary path.
pub const OPENCODE_BINARY_ENV: &str = "OPENCODE_BINARY";

/// Environment variable for extra arguments between binary and `serve` command.
///
/// Useful for launchers like `bunx` where the full command is:
/// `bunx --yes opencode-ai@1.14.19 serve --hostname ... --port ...`
///
/// Example: `OPENCODE_BINARY=bunx OPENCODE_BINARY_ARGS="--yes opencode-ai@1.14.19"`
pub const OPENCODE_BINARY_ARGS_ENV: &str = "OPENCODE_BINARY_ARGS";

/// Normalize a version string by stripping the `v` prefix if present.
pub fn normalize_version(raw: &str) -> &str {
    let trimmed = raw.trim();
    trimmed.strip_prefix('v').unwrap_or(trimmed)
}

/// Validate that the actual version matches the pinned version exactly.
///
/// # Errors
///
/// Returns an error if the version is missing or doesn't match.
pub fn validate_exact_version(actual: Option<&str>) -> Result<()> {
    let Some(actual) = actual else {
        return Err(OpencodeError::VersionMismatch {
            expected: PINNED_OPENCODE_VERSION.to_string(),
            actual: "None".to_string(),
        });
    };

    let normalized = normalize_version(actual);
    if normalized != PINNED_OPENCODE_VERSION {
        return Err(OpencodeError::VersionMismatch {
            expected: PINNED_OPENCODE_VERSION.to_string(),
            actual: actual.to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_version_strips_v_prefix() {
        assert_eq!(normalize_version("v1.14.19"), "1.14.19");
        assert_eq!(normalize_version("1.14.19"), "1.14.19");
        assert_eq!(normalize_version("  v1.14.19  "), "1.14.19");
    }

    #[test]
    fn test_validate_exact_version_accepts_matching() {
        assert!(validate_exact_version(Some(PINNED_OPENCODE_VERSION)).is_ok());
        assert!(validate_exact_version(Some(&format!("v{PINNED_OPENCODE_VERSION}"))).is_ok());
    }

    #[test]
    fn test_validate_exact_version_rejects_mismatch() {
        assert!(validate_exact_version(Some("1.14.18")).is_err());
        assert!(validate_exact_version(Some("1.15.0")).is_err());
        assert!(validate_exact_version(None).is_err());
    }
}
