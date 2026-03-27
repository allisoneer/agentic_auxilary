//! httpmock-backed proxy/record/playback utilities for conformance tests.
//!
//! Modes (driven by env vars, interpreted by `snapshots.rs`):
//! - Replay (default): start server + `playback()` from YAML
//! - Live + record: start server + `forward_to()` upstream + `record()` + save YAML on drop
//!
//! This module owns:
//! - snapshot file paths
//! - API key and sensitive header redaction
//! - starting/stopping the httpmock server used by the harness

use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use httpmock::MockServer;
use httpmock::Recording;

/// `ANTHROPIC_LIVE=1` => run against the real API (optionally via proxy when recording).
pub const ENV_LIVE: &str = "ANTHROPIC_LIVE";
/// `ANTHROPIC_RECORD=1` => in live mode, record YAML to disk.
pub const ENV_RECORD: &str = "ANTHROPIC_RECORD";
/// Real API key used only in live mode.
pub const ENV_API_KEY: &str = "ANTHROPIC_API_KEY";
/// Optional override for where snapshots are stored.
pub const ENV_SNAPSHOT_DIR: &str = "ANTHROPIC_SNAPSHOT_DIR";

/// Default upstream Anthropic API base URL.
pub const DEFAULT_UPSTREAM_BASE: &str = "https://api.anthropic.com";

/// Response headers to redact from cassette recordings.
/// These contain identity info or noise that shouldn't be in version control.
const HEADERS_TO_REDACT: &[&str] = &[
    "anthropic-organization-id", // Links repo to specific Anthropic account
    "request-id",                // Noise, changes every request
    "cf-ray",                    // Cloudflare ray ID, noise + datacenter info
];

/// True if running in live mode (`ANTHROPIC_LIVE=1`).
#[must_use]
pub fn is_live() -> bool {
    env::var(ENV_LIVE).as_deref() == Ok("1")
}

/// True if recording is enabled (`ANTHROPIC_RECORD=1`).
#[must_use]
pub fn is_recording() -> bool {
    env::var(ENV_RECORD).as_deref() == Ok("1")
}

/// Default snapshot directory: `$CARGO_MANIFEST_DIR/tests/snapshots`.
#[must_use]
pub fn default_snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
}

/// Snapshot directory, overridable via `ANTHROPIC_SNAPSHOT_DIR`.
#[must_use]
pub fn snapshot_dir() -> PathBuf {
    env::var(ENV_SNAPSHOT_DIR).map_or_else(|_| default_snapshot_dir(), PathBuf::from)
}

/// Recording file path for a given test name (httpmock YAML).
#[must_use]
pub fn recording_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.yaml"))
}

/// Server handle kept alive for the duration of a test.
///
/// - In replay mode: owns `MockServer` with `playback(...)` loaded.
/// - In record mode: owns `MockServer`, saves YAML on Drop.
///
/// We store only the recording ID (not the `Recording<'a>` handle) to avoid
/// self-referential struct issues. The Recording handle is reconstructed
/// in Drop when we need to save.
pub struct SnapshotServer {
    server: MockServer,
    /// Recording ID (if recording). We store just the ID to avoid self-referential lifetimes.
    recording_id: Option<usize>,
    snapshot_dir: PathBuf,
    name: String,
    redact_api_key: Option<String>,
}

impl SnapshotServer {
    /// Start a server in playback mode from an existing YAML recording.
    ///
    /// # Panics
    /// Panics if the recording file does not exist.
    pub async fn start_playback(name: &str) -> Self {
        let dir = snapshot_dir();
        let path = recording_path(&dir, name);

        assert!(
            path.exists(),
            "Missing snapshot recording: {}\n\
             Record it with: ANTHROPIC_LIVE=1 ANTHROPIC_RECORD=1 {}=... cargo test -p anthropic-async {} -- --nocapture",
            path.display(),
            ENV_API_KEY,
            name
        );

        let server = MockServer::start_async().await;
        server.playback(&path);

        Self {
            server,
            recording_id: None,
            snapshot_dir: dir,
            name: name.to_string(),
            redact_api_key: None,
        }
    }

    /// Start a proxy server that forwards to `upstream_base`.
    /// If `record=true`, it records interactions and saves them on drop.
    ///
    /// `upstream_api_key` is used only for forwarding; it should not end up on disk.
    pub async fn start_live_proxy(
        name: &str,
        upstream_base: &str,
        upstream_api_key: String,
        record: bool,
    ) -> Self {
        let dir = snapshot_dir();
        let server = MockServer::start_async().await;

        // Set up forwarding to upstream with the real API key
        let key_clone = upstream_api_key.clone();
        server.forward_to(upstream_base, |rule| {
            rule.add_request_header("x-api-key", key_clone);
        });

        // Start recording if requested, but only store the ID (not the Recording handle)
        // to avoid self-referential struct issues. We'll reconstruct the handle in Drop.
        let recording_id = if record {
            let recording = server.record(|rule| {
                // Record all requests, including relevant headers
                rule.record_request_headers(vec![
                    "content-type",
                    "anthropic-version",
                    "anthropic-beta",
                ]);
            });
            Some(recording.id)
        } else {
            None
        };

        Self {
            server,
            recording_id,
            snapshot_dir: dir,
            name: name.to_string(),
            redact_api_key: Some(upstream_api_key),
        }
    }

    /// Base URL of the local server (use as `api_base` for the SDK client).
    #[must_use]
    pub fn base_url(&self) -> String {
        self.server.base_url()
    }

    /// Save the recording to disk. Called automatically in Drop.
    fn save_recording(&self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(id) = self.recording_id else {
            return Ok(());
        };

        // Create snapshot directory if needed
        fs::create_dir_all(&self.snapshot_dir)?;

        // Reconstruct the Recording handle from the ID.
        // This is safe because we still own the server and it hasn't been dropped.
        let recording = Recording::new(id, &self.server);

        // Save the recording (httpmock adds a timestamp to the filename)
        recording.save_to(&self.snapshot_dir, &self.name)?;

        // Post-process: redact API key and rename to canonical path
        let canonical_path = recording_path(&self.snapshot_dir, &self.name);
        self.postprocess_recording(&canonical_path)?;

        Ok(())
    }

    /// Find the newest YAML file, redact the API key, and rename to canonical path.
    fn postprocess_recording(
        &self,
        canonical_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(key) = self.redact_api_key.as_deref() else {
            return Ok(());
        };

        // Find the most recent yaml file matching our name pattern
        // (httpmock adds timestamps to filenames)
        let entries = fs::read_dir(&self.snapshot_dir)?;
        let mut matching_files: Vec<_> = entries
            .filter_map(Result::ok)
            .filter(|e| {
                let path = e.path();
                // Use underscore delimiter to prevent prefix collisions
                // (e.g., "multi_turn" should not match "multi_turn_v2_*.yaml")
                let expected_prefix = format!("{}_", self.name);
                let name_matches = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&expected_prefix));
                let ext_matches = path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("yaml"));
                name_matches && ext_matches
            })
            .collect();

        // Sort by modification time, newest first
        matching_files.sort_by(|a, b| {
            b.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .cmp(
                    &a.metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                )
        });

        if let Some(newest) = matching_files.first() {
            let newest_path = newest.path();

            // Redact the API key from the file
            redact_string_in_file(&newest_path, key, "<redacted>")?;

            // Redact sensitive response headers (org-id, request-id, cf-ray)
            for header in HEADERS_TO_REDACT {
                redact_header_value(&newest_path, header, "<redacted>")?;
            }

            // Rename to the canonical path (without timestamp) for easy playback
            if newest_path != canonical_path {
                fs::rename(&newest_path, canonical_path)?;
            }
        }

        Ok(())
    }
}

impl Drop for SnapshotServer {
    fn drop(&mut self) {
        // Skip saving if no recording was made
        if self.recording_id.is_none() {
            return;
        }

        // Don't try to save if test is already failing - httpmock may panic
        // if there are no recorded interactions
        if std::thread::panicking() {
            eprintln!("Test failed, skipping cassette save for '{}'", self.name);
            return;
        }

        // Save the recording
        if let Err(e) = self.save_recording() {
            eprintln!("Failed to save recording '{}': {e}", self.name);
        }
    }
}

fn redact_string_in_file(path: &Path, needle: &str, replacement: &str) -> std::io::Result<()> {
    let before = fs::read_to_string(path)?;
    let after = before.replace(needle, replacement);
    if after != before {
        fs::write(path, after)?;
    }
    Ok(())
}

/// Redact a response header value by name in an httpmock YAML cassette.
///
/// Finds lines matching `- name: {header_name}` and replaces the value on the
/// following `value:` line with the replacement string.
fn redact_header_value(path: &Path, header_name: &str, replacement: &str) -> std::io::Result<()> {
    let content = fs::read_to_string(path)?;
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let mut modified = false;

    let mut i = 0;
    while i < lines.len() {
        // Check if this line declares the header we're looking for
        // YAML format: "  - name: header-name"
        let is_target_header = {
            let trimmed = lines[i].trim();
            trimmed.starts_with("- name:")
                && trimmed.strip_prefix("- name:").is_some_and(|rest| {
                    rest.trim()
                        .trim_matches(|c| c == '"' || c == '\'')
                        .eq_ignore_ascii_case(header_name)
                })
        };

        if is_target_header && i + 1 < lines.len() {
            // Next line should be the value line: "    value: some-value"
            if let Some(colon_pos) = lines[i + 1].find("value:") {
                let prefix_end = colon_pos + 6; // "value:" is 6 chars
                let prefix = &lines[i + 1][..prefix_end];
                lines[i + 1] = format!("{prefix} {replacement}");
                modified = true;
            }
        }
        i += 1;
    }

    if modified {
        let mut new_content = lines.join("\n");
        // Preserve trailing newline if original had one
        if content.ends_with('\n') {
            new_content.push('\n');
        }
        fs::write(path, new_content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_live_default() {
        // By default (no env var), should be replay mode
        if env::var(ENV_LIVE).is_err() {
            assert!(!is_live());
        }
    }

    #[test]
    fn test_is_recording_default() {
        // By default (no env var), should not be recording
        if env::var(ENV_RECORD).is_err() {
            assert!(!is_recording());
        }
    }

    #[test]
    fn test_default_snapshot_dir_is_valid() {
        let dir = default_snapshot_dir();
        assert!(dir.ends_with("tests/snapshots"));
    }

    #[test]
    fn test_recording_path_formats_correctly() {
        let dir = PathBuf::from("/tmp/snapshots");
        let path = recording_path(&dir, "multi_turn_tool_conversation");
        assert_eq!(
            path,
            PathBuf::from("/tmp/snapshots/multi_turn_tool_conversation.yaml")
        );
    }

    #[tokio::test]
    async fn test_playback_server_panics_on_missing_recording() {
        // Use a name that definitely doesn't exist
        let result = std::panic::catch_unwind(|| {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(SnapshotServer::start_playback("nonexistent_test_12345"));
        });
        assert!(result.is_err(), "Expected panic for missing recording");
    }

    #[test]
    fn test_redact_header_value() {
        use std::io::Write;

        // Create a temp file with YAML content similar to httpmock cassettes
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        let yaml_content = r"then:
  status: 200
  header:
  - name: content-type
    value: application/json
  - name: anthropic-organization-id
    value: 74e30b4a-5fab-438b-8fe7-5989330fe3b2
  - name: request-id
    value: req_011CYTicPYkV5sJsYj1oKGUr
  - name: cf-ray
    value: 9d328cbcce781376-DFW
";
        temp.write_all(yaml_content.as_bytes()).unwrap();
        temp.flush().unwrap();

        // Redact the org-id header
        redact_header_value(temp.path(), "anthropic-organization-id", "<redacted>").unwrap();

        let result = fs::read_to_string(temp.path()).unwrap();

        // Org-id should be redacted
        assert!(result.contains("- name: anthropic-organization-id"));
        assert!(result.contains("value: <redacted>"));
        assert!(!result.contains("74e30b4a-5fab-438b-8fe7-5989330fe3b2"));

        // Other headers should be unchanged
        assert!(result.contains("value: application/json"));
        assert!(result.contains("value: req_011CYTicPYkV5sJsYj1oKGUr"));
        assert!(result.contains("value: 9d328cbcce781376-DFW"));
    }

    #[test]
    fn test_redact_header_value_multiple_occurrences() {
        use std::io::Write;

        // Multi-turn cassette has the same headers in multiple responses
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        let yaml_content = r"---
then:
  header:
  - name: anthropic-organization-id
    value: first-org-id
---
then:
  header:
  - name: anthropic-organization-id
    value: second-org-id
";
        temp.write_all(yaml_content.as_bytes()).unwrap();
        temp.flush().unwrap();

        redact_header_value(temp.path(), "anthropic-organization-id", "<redacted>").unwrap();

        let result = fs::read_to_string(temp.path()).unwrap();

        // Both occurrences should be redacted
        assert!(!result.contains("first-org-id"));
        assert!(!result.contains("second-org-id"));
        assert_eq!(result.matches("value: <redacted>").count(), 2);
    }

    #[test]
    fn test_redact_header_value_case_insensitive() {
        use std::io::Write;

        // HTTP headers are case-insensitive per RFC 7230; verify redaction handles this
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        let yaml_content = r#"then:
  status: 200
  header:
  - name: Request-Id
    value: req_mixed_case_123
  - name: CF-RAY
    value: uppercase-cf-ray-456
  - name: "Anthropic-Organization-Id"
    value: quoted-org-id-789
"#;
        temp.write_all(yaml_content.as_bytes()).unwrap();
        temp.flush().unwrap();

        // Redact using lowercase names (as defined in HEADERS_TO_REDACT)
        redact_header_value(temp.path(), "request-id", "<redacted>").unwrap();
        redact_header_value(temp.path(), "cf-ray", "<redacted>").unwrap();
        redact_header_value(temp.path(), "anthropic-organization-id", "<redacted>").unwrap();

        let result = fs::read_to_string(temp.path()).unwrap();

        // All values should be redacted despite case differences
        assert!(!result.contains("req_mixed_case_123"));
        assert!(!result.contains("uppercase-cf-ray-456"));
        assert!(!result.contains("quoted-org-id-789"));
        assert_eq!(result.matches("value: <redacted>").count(), 3);
    }
}
