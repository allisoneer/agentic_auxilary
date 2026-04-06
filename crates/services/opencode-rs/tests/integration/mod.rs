//! Integration tests for `opencode_rs`.
//!
//! These tests verify typed deserialization against a live opencode server.
//!
//! # Running the tests
//!
//! With `server` feature (recommended - auto-spawns server):
//!   The parent `integration.rs` module starts a `ManagedServer` and sets
//!   `OPENCODE_BASE_URL` to its dynamic port. These tests then use that URL.
//!
//! Without `server` feature (manual server):
//!   1. Start the opencode server: `opencode serve --port 4096 --hostname 127.0.0.1`
//!   2. Set the environment variable: `OPENCODE_INTEGRATION=1`
//!   3. Optionally set `OPENCODE_BASE_URL` (defaults to `http://127.0.0.1:4096`)
//!   4. Run the tests: `cargo test --test integration -- --ignored`

// Integration tests are allowed to use unwrap/expect for test assertions
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod http_endpoints;
mod server_sse;

/// Check if integration tests should run.
pub fn should_run() -> bool {
    std::env::var("OPENCODE_INTEGRATION").is_ok()
}

/// Get the test server URL.
///
/// When running with the `server` feature, the parent `integration.rs` module
/// sets `OPENCODE_BASE_URL` after starting the managed server. This allows
/// these typed tests to connect to the same server instance.
pub fn test_url() -> String {
    std::env::var("OPENCODE_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:4096".to_string())
}

/// Create a test client connected to the test server.
pub async fn create_test_client() -> opencode_rs::Client {
    opencode_rs::Client::builder()
        .base_url(test_url())
        .timeout_secs(30)
        .build()
        .unwrap()
}
