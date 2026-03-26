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

use anthropic_async::AnthropicConfig;
use anthropic_async::Client;

use super::recording::SnapshotServer;
use super::recording::{self};

/// Check if we're running in live mode (real API calls).
#[must_use]
pub fn is_live() -> bool {
    recording::is_live()
}

/// Snapshot test harness that supports both live and replay modes.
pub struct SnapshotHarness {
    /// The configured client (either real or mock-backed).
    client: Client<AnthropicConfig>,
    /// The snapshot server (mock for replay, proxy for record, None for live).
    /// Kept alive for the test duration; Drop saves recordings in record mode.
    server: Option<SnapshotServer>,
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
                Self::new_live()
            }
        } else {
            Self::new_replay(name).await
        }
    }

    /// Create a harness in live mode (direct real API calls, no recording).
    #[expect(
        clippy::expect_used,
        reason = "live mode requires API key; descriptive panic message guides the user"
    )]
    fn new_live() -> Self {
        let api_key = env::var(recording::ENV_API_KEY).expect(
            "ANTHROPIC_API_KEY required when ANTHROPIC_LIVE=1 (set ANTHROPIC_RECORD=1 to record)",
        );

        let config = AnthropicConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);

        Self {
            client,
            server: None,
        }
    }

    /// Create a harness in live+record mode (proxy with recording).
    #[expect(
        clippy::expect_used,
        reason = "live+record mode requires API key; descriptive panic message guides the user"
    )]
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
            server: Some(server),
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
            server: Some(server),
        }
    }

    /// Get a reference to the configured client.
    #[must_use]
    pub const fn client(&self) -> &Client<AnthropicConfig> {
        &self.client
    }

    /// Get the base URL for API requests.
    ///
    /// In replay/record modes, returns the mock server URL.
    /// In live mode, returns the real Anthropic API URL.
    #[must_use]
    pub fn base_url(&self) -> String {
        self.server.as_ref().map_or_else(
            || recording::DEFAULT_UPSTREAM_BASE.to_string(),
            SnapshotServer::base_url,
        )
    }

    /// Check if running in live mode (direct API calls).
    ///
    /// This validates that the harness configuration is consistent with the environment:
    /// - Live mode without recording: no server (direct API)
    /// - Live mode with recording: has proxy server
    /// - Replay mode: has mock server
    #[must_use]
    pub fn is_live(&self) -> bool {
        let live = is_live();
        // Validate configuration consistency (uses self.server meaningfully)
        debug_assert!(
            live || self.server.is_some(),
            "Replay mode should have a mock server"
        );
        live
    }

    /// Check if using a mock/proxy server (replay or record mode).
    #[must_use]
    pub const fn has_server(&self) -> bool {
        self.server.is_some()
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
