//! Integration tests for opencode_rs.
//!
//! These tests verify typed deserialization against a live opencode server.
//!
//! # Running the tests
//!
//! 1. Start the opencode server: `opencode serve --port 4096 --hostname 127.0.0.1`
//! 2. Set the environment variable: `OPENCODE_INTEGRATION=1`
//! 3. Optionally set `OPENCODE_BASE_URL` (defaults to `http://127.0.0.1:4096`)
//! 4. Run the tests: `cargo test --test integration -- --ignored`

mod http_endpoints;
mod server_sse;

/// Check if integration tests should run.
pub fn should_run() -> bool {
    std::env::var("OPENCODE_INTEGRATION").is_ok()
}

/// Get the test server URL.
pub fn test_url() -> String {
    std::env::var("OPENCODE_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:4096".to_string())
}

/// Create a test client.
pub async fn create_test_client() -> opencode_rs::Client {
    opencode_rs::Client::builder()
        .base_url(test_url())
        .timeout_secs(30)
        .build()
        .expect("Failed to create test client")
}
