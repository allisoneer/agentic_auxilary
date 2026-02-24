//! Snapshot test harness for live/replay conformance testing.
//!
//! This module provides infrastructure for testing against the Anthropic API
//! with support for both live and replay modes:
//!
//! - **Replay mode** (default): Uses httpmock to replay recorded snapshots.
//!   This is deterministic and doesn't require an API key, making it suitable for CI.
//!
//! - **Live mode** (`ANTHROPIC_LIVE=1`): Makes real API calls. If `ANTHROPIC_RECORD=1`
//!   is also set, records the interactions to YAML for later replay.
//!
//! # Usage
//!
//! ```ignore
//! use crate::support::snapshots::SnapshotHarness;
//!
//! #[tokio::test]
//! async fn test_multi_turn() {
//!     let harness = SnapshotHarness::new("multi_turn").await;
//!     let client = harness.client();
//!     // ... run tests ...
//! }
//! ```

use std::env;

use anthropic_async::{AnthropicConfig, Client};

use super::recording::{self, SnapshotServer};

/// Check if we're running in live mode (real API calls).
#[must_use]
pub fn is_live() -> bool {
    recording::is_live()
}

/// Snapshot test harness that supports both live and replay modes.
pub struct SnapshotHarness {
    /// The configured client (either real or mock-backed).
    client: Client<AnthropicConfig>,
    /// Keeps the server alive for replay/record mode; Drop will also save recordings.
    _server: Option<SnapshotServer>,
    /// Name of this test (used for snapshot file naming).
    #[allow(dead_code)] // Will be used for insta snapshots in Phase 6
    name: String,
}

impl SnapshotHarness {
    /// Create a new harness for the given test name.
    ///
    /// In replay mode, this starts an httpmock server with playback from YAML.
    /// In live mode without recording, this creates a direct client.
    /// In live+record mode, this creates a proxy server that records interactions.
    pub async fn new(name: &str) -> Self {
        if recording::is_live() {
            if recording::is_recording() {
                Self::new_live_record(name).await
            } else {
                Self::new_live(name)
            }
        } else {
            Self::new_replay(name).await
        }
    }

    /// Create a harness in live mode (direct real API calls, no recording).
    fn new_live(name: &str) -> Self {
        let api_key = env::var(recording::ENV_API_KEY).expect(
            "ANTHROPIC_API_KEY required when ANTHROPIC_LIVE=1 (set ANTHROPIC_RECORD=1 to record)",
        );

        let config = AnthropicConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);

        Self {
            client,
            _server: None,
            name: name.to_string(),
        }
    }

    /// Create a harness in live+record mode (proxy with recording).
    async fn new_live_record(name: &str) -> Self {
        let upstream_api_key = env::var(recording::ENV_API_KEY)
            .expect("ANTHROPIC_API_KEY required when ANTHROPIC_LIVE=1 and ANTHROPIC_RECORD=1");

        let server = SnapshotServer::start_live_proxy(
            name,
            recording::DEFAULT_UPSTREAM_BASE,
            upstream_api_key,
            true,
        )
        .await;

        // Skip auth - the proxy injects the real API key when forwarding.
        // This ensures no auth headers are sent by the client, avoiding duplicate headers.
        let config = AnthropicConfig::new()
            .dangerously_skip_auth()
            .with_api_base(server.base_url());
        let client = Client::with_config(config);

        Self {
            client,
            _server: Some(server),
            name: name.to_string(),
        }
    }

    /// Create a harness in replay mode (mock server with playback).
    async fn new_replay(name: &str) -> Self {
        let server = SnapshotServer::start_playback(name).await;

        // Skip auth - replay mode doesn't need real credentials.
        // This matches record mode so cassettes are consistent.
        let config = AnthropicConfig::new()
            .dangerously_skip_auth()
            .with_api_base(server.base_url());
        let client = Client::with_config(config);

        Self {
            client,
            _server: Some(server),
            name: name.to_string(),
        }
    }

    /// Get a reference to the configured client.
    #[must_use]
    pub const fn client(&self) -> &Client<AnthropicConfig> {
        &self.client
    }

    /// Check if running in live mode.
    #[must_use]
    #[allow(clippy::unused_self)]
    #[allow(dead_code)] // Public API for tests
    pub fn is_live(&self) -> bool {
        is_live()
    }

    /// Get the test name.
    #[must_use]
    #[allow(dead_code)] // Public API for tests
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_live_default() {
        // By default (no env var), should be replay mode
        if env::var(recording::ENV_LIVE).is_err() {
            assert!(!is_live());
        }
    }
}
