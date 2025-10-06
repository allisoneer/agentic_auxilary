use anyhow::{Result, bail};
use std::path::{Component, Path};

/// Validate a simple filename: no directories, no traversal, not absolute.
/// Allows [A-Za-z0-9._-] only, must not be empty.
pub fn validate_simple_filename(filename: &str) -> Result<()> {
    if filename.trim().is_empty() {
        bail!("Filename cannot be empty");
    }

    // Parse as path and check components
    let p = Path::new(filename);
    let mut comps = p.components();

    // Reject absolute paths
    if matches!(
        comps.next(),
        Some(Component::RootDir | Component::Prefix(_))
    ) {
        bail!("Absolute paths are not allowed");
    }

    // Must be single component (no directories)
    if p.components().count() != 1 {
        bail!("Filename must not contain directories");
    }

    // Reject special names
    if filename == "." || filename == ".." {
        bail!("Invalid filename");
    }

    // Restrict to safe character set
    let ok = filename
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !ok {
        bail!("Filename contains invalid characters (allowed: A-Z a-z 0-9 . _ -)");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_simple_filename_ok() {
        for f in ["a.md", "plan-01.md", "notes_v2.md", "R1.TOC"] {
            assert!(validate_simple_filename(f).is_ok(), "{f}");
        }
    }

    #[test]
    fn test_validate_simple_filename_bad() {
        for f in [
            "../x.md",
            "/abs.md",
            "a/b.md",
            " ",
            "",
            ".",
            "..",
            "name with space.md",
        ] {
            assert!(validate_simple_filename(f).is_err(), "{f}");
        }
    }
}
