//! httpmock-backed proxy/record/playback utilities for conformance tests.
//!
//! Modes (driven by env vars, interpreted by `snapshots.rs`):
//! - Replay (default): start server + `playback()` from YAML
//! - Live + record: start server + `forward_to()` upstream + `record()` + save YAML on drop
//!
//! This module owns:
//! - snapshot file paths
//! - API key redaction (safety net)
//! - starting/stopping the httpmock server used by the harness

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use httpmock::{MockServer, Recording};

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
/// - In record mode: owns `MockServer` + recording handle, saves YAML on Drop.
pub struct SnapshotServer {
    // Note: field order matters! recording must be dropped before server.
    // Recording holds a reference to MockServer, so server must outlive recording.
    recording: Option<Recording<'static>>,
    server: MockServer,
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
            recording: None,
            server,
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

        let recording = if record {
            let rec = server.record(|rule| {
                // Record all requests, including relevant headers
                rule.record_request_headers(vec![
                    "content-type",
                    "anthropic-version",
                    "anthropic-beta",
                ]);
            });
            // SAFETY: We own `server` and it lives as long as this struct.
            // Recording only holds a reference to server, so extending to 'static
            // is safe as long as recording is dropped before server (which Rust
            // guarantees due to field declaration order).
            Some(unsafe { std::mem::transmute::<Recording<'_>, Recording<'static>>(rec) })
        } else {
            None
        };

        Self {
            recording,
            server,
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
}

impl Drop for SnapshotServer {
    fn drop(&mut self) {
        let Some(recording) = self.recording.take() else {
            return;
        };

        // Never double-panic (a failing test + failing save would abort).
        let already_panicking = std::thread::panicking();

        // Create snapshot directory if needed
        if let Err(err) = fs::create_dir_all(&self.snapshot_dir) {
            if already_panicking {
                eprintln!("Failed to create snapshot dir: {err}");
                return;
            }
            panic!("Failed to create snapshot dir: {err}");
        }

        // Save the recording
        let path = recording_path(&self.snapshot_dir, &self.name);

        if let Err(err) = recording.save_to(&self.snapshot_dir, &self.name) {
            if already_panicking {
                eprintln!("Failed to save recording: {err}");
                return;
            }
            panic!("Failed to save recording: {err}");
        }

        // Safety net: redact the real API key if it ended up serialized anywhere.
        // httpmock's save_to creates a file with timestamp, we need to find it and rename it
        if let Some(key) = self.redact_api_key.as_deref() {
            // Find the most recent yaml file matching our name pattern
            if let Ok(entries) = fs::read_dir(&self.snapshot_dir) {
                let mut matching_files: Vec<_> = entries
                    .filter_map(Result::ok)
                    .filter(|e| {
                        let path = e.path();
                        let name_matches = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with(&self.name));
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
                    if let Err(err) = redact_string_in_file(&newest_path, key, "<redacted>") {
                        if already_panicking {
                            eprintln!(
                                "Failed to redact API key from {}: {err}",
                                newest_path.display()
                            );
                            return;
                        }
                        panic!(
                            "Failed to redact API key from {}: {err}",
                            newest_path.display()
                        );
                    }

                    // Rename to the canonical path (without timestamp) for easy playback
                    if newest_path != path
                        && let Err(err) = fs::rename(&newest_path, &path)
                    {
                        if already_panicking {
                            eprintln!(
                                "Failed to rename {} to {}: {err}",
                                newest_path.display(),
                                path.display()
                            );
                            return;
                        }
                        panic!(
                            "Failed to rename {} to {}: {err}",
                            newest_path.display(),
                            path.display()
                        );
                    }
                }
            }
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
}
