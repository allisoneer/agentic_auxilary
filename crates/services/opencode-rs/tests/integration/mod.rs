//! Integration tests for `opencode_rs`.
//!
//! These tests verify typed deserialization against a live opencode server.
//!
//! # Running the tests
//!
//! With `server` feature (recommended - auto-spawns server):
//!   The parent `integration.rs` module starts a `ManagedServer`. These tests
//!   call `super::start_server()` directly to ensure the server is initialized
//!   before obtaining the URL.
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

// ============================================================================
// Server feature enabled: Get URL from managed server directly
// ============================================================================

/// Create a test client connected to the managed test server.
///
/// This calls `super::start_server()` to ensure the managed server is started,
/// then uses its URL directly. This removes the dependency on test execution
/// order that previously existed when relying on `OPENCODE_BASE_URL` being
/// pre-populated by a parent test.
#[cfg(feature = "server")]
pub async fn create_test_client() -> opencode_rs::Client {
    let server = super::start_server().await;
    opencode_rs::Client::builder()
        .base_url(server.url().as_str())
        .timeout_secs(30)
        .build()
        .unwrap()
}

// ============================================================================
// Server feature disabled: Connect to pre-running server via env var
// ============================================================================

/// Get the test server URL from environment.
#[cfg(not(feature = "server"))]
fn test_url() -> String {
    std::env::var("OPENCODE_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:4096".to_string())
}

/// Create a test client connected to a pre-running test server.
#[cfg(not(feature = "server"))]
pub async fn create_test_client() -> opencode_rs::Client {
    opencode_rs::Client::builder()
        .base_url(test_url())
        .timeout_secs(30)
        .build()
        .unwrap()
}
