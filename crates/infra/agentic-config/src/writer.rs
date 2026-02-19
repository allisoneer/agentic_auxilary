//! Atomic file writing for configuration files.
//!
//! Uses the atomicwrites crate to ensure config files are either fully written
//! or not modified at all, preventing corruption from partial writes.

use anyhow::{Context, Result};
use atomicwrites::{AllowOverwrite, AtomicFile};
use serde_json::Value;
use std::io::Write;
use std::path::Path;

/// Write a JSON Value to a file atomically with pretty formatting.
///
/// The file will be written to a temporary location first, then atomically
/// renamed to the target path. This ensures the file is never left in a
/// partial state.
pub fn write_pretty_json_atomic(path: &Path, value: &Value) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let json = serde_json::to_string_pretty(value).context("Failed to serialize config to JSON")?;

    let af = AtomicFile::new(path, AllowOverwrite);
    af.write(|f| f.write_all(json.as_bytes()))
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn test_write_creates_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.json");

        let value = json!({"key": "value"});
        write_pretty_json_atomic(&path, &value).unwrap();

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"key\""));
        assert!(content.contains("\"value\""));
    }

    #[test]
    fn test_write_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nested").join("dir").join("test.json");

        let value = json!({"nested": true});
        write_pretty_json_atomic(&path, &value).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_write_overwrites_existing() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.json");

        // Write initial value
        let value1 = json!({"version": 1});
        write_pretty_json_atomic(&path, &value1).unwrap();

        // Overwrite
        let value2 = json!({"version": 2});
        write_pretty_json_atomic(&path, &value2).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"version\": 2"));
        assert!(!content.contains("\"version\": 1"));
    }
}
