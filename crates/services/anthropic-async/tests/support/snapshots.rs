//! Snapshot test harness for live/replay conformance testing.
//!
//! This module provides infrastructure for testing against the Anthropic API
//! with support for both live and replay modes:
//!
//! - **Replay mode** (default): Uses wiremock to replay recorded snapshots.
//!   This is deterministic and doesn't require an API key, making it suitable for CI.
//!
//! - **Live mode** (`ANTHROPIC_LIVE=1`): Makes real API calls and can record
//!   new snapshots. Requires `ANTHROPIC_API_KEY` environment variable.
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
use wiremock::MockServer;

/// Check if we're running in live mode (real API calls).
#[must_use]
pub fn is_live() -> bool {
    env::var("ANTHROPIC_LIVE").as_deref() == Ok("1")
}

/// Snapshot test harness that supports both live and replay modes.
pub struct SnapshotHarness {
    /// The configured client (either real or mock-backed).
    client: Client<AnthropicConfig>,
    /// The mock server (only used in replay mode).
    _mock_server: Option<MockServer>,
    /// Name of this test (used for snapshot file naming).
    #[allow(dead_code)]
    name: String,
}

impl SnapshotHarness {
    /// Create a new harness for the given test name.
    ///
    /// In replay mode, this starts a wiremock server.
    /// In live mode, this creates a client with the real API.
    pub async fn new(name: &str) -> Self {
        if is_live() {
            Self::new_live(name)
        } else {
            Self::new_replay(name).await
        }
    }

    /// Create a harness in live mode (real API calls).
    fn new_live(name: &str) -> Self {
        let api_key = env::var("ANTHROPIC_API_KEY")
            .expect("ANTHROPIC_API_KEY required when ANTHROPIC_LIVE=1");

        let config = AnthropicConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);

        Self {
            client,
            _mock_server: None,
            name: name.to_string(),
        }
    }

    /// Create a harness in replay mode (mock server).
    async fn new_replay(name: &str) -> Self {
        let server = MockServer::start().await;

        // In replay mode, we load pre-recorded snapshots.
        // For now, we create a client that points to the mock server.
        // Actual snapshot loading would be implemented here.
        let config = AnthropicConfig::new()
            .with_api_key("test-key")
            .with_api_base(server.uri());
        let client = Client::with_config(config);

        Self {
            client,
            _mock_server: Some(server),
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
    #[allow(clippy::unused_self)] // Provides convenient method API for tests
    pub fn is_live(&self) -> bool {
        is_live()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_live_default() {
        // By default (no env var), should be replay mode
        // Note: This test might be flaky if ANTHROPIC_LIVE is set in the environment
        // In practice, CI won't have it set
        if env::var("ANTHROPIC_LIVE").is_err() {
            assert!(!is_live());
        }
    }

    #[tokio::test]
    async fn test_harness_initializes_replay() {
        // Skip if running in live mode
        if is_live() {
            return;
        }

        let harness = SnapshotHarness::new("test_init").await;
        assert!(!harness.is_live());
        // Client should be configured
        let _ = harness.client();
    }
}
