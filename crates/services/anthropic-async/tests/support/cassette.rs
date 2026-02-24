//! Cassette types and disk IO utilities for conformance test recording/replay.
//!
//! Cassettes store a sequence of API interactions (request/response pairs) that can be
//! replayed in tests for deterministic CI without requiring API keys.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// A cassette containing a sequence of recorded API interactions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub struct Cassette {
    /// The recorded interactions in order.
    pub interactions: Vec<Interaction>,
}

/// A single recorded request/response pair.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub struct Interaction {
    /// The recorded request.
    pub request: RecordedRequest,
    /// The recorded response.
    pub response: RecordedResponse,
}

/// A recorded HTTP request.
// Note: Can't derive Eq because serde_json::Value contains f64
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub struct RecordedRequest {
    /// HTTP method (e.g., "POST").
    pub method: String,
    /// Request path (e.g., "/v1/messages").
    pub path: String,
    /// Request body as JSON (None for bodyless requests).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

/// A recorded HTTP response.
// Note: Can't derive Eq because serde_json::Value contains f64
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub struct RecordedResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body as JSON.
    pub body: serde_json::Value,
}

/// Returns the default snapshot directory for cassettes.
///
/// This is `$CARGO_MANIFEST_DIR/tests/snapshots`.
#[must_use]
pub fn default_snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
}

/// Returns the path for a cassette with the given name.
#[must_use]
pub fn cassette_path(snapshot_dir: &Path, name: &str) -> PathBuf {
    snapshot_dir.join(format!("{name}.json"))
}

/// Load a cassette from disk.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
#[allow(dead_code)] // Used in Phase 4 when snapshots.rs imports this
pub fn load(path: &Path) -> std::io::Result<Cassette> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Save a cassette to disk with pretty JSON formatting.
///
/// Creates parent directories if needed.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
#[allow(dead_code)] // Used in Phase 4 when snapshots.rs imports this
pub fn save(path: &Path, cassette: &Cassette) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(cassette)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cassette_roundtrips() {
        let cassette = Cassette {
            interactions: vec![Interaction {
                request: RecordedRequest {
                    method: "POST".into(),
                    path: "/v1/messages".into(),
                    body: Some(serde_json::json!({"hello": "world"})),
                },
                response: RecordedResponse {
                    status: 200,
                    body: serde_json::json!({"ok": true}),
                },
            }],
        };

        let s = serde_json::to_string(&cassette).unwrap();
        let parsed: Cassette = serde_json::from_str(&s).unwrap();
        assert_eq!(cassette, parsed);
    }

    #[test]
    fn cassette_with_no_body_serializes_cleanly() {
        let cassette = Cassette {
            interactions: vec![Interaction {
                request: RecordedRequest {
                    method: "GET".into(),
                    path: "/v1/models".into(),
                    body: None,
                },
                response: RecordedResponse {
                    status: 200,
                    body: serde_json::json!({"models": []}),
                },
            }],
        };

        let s = serde_json::to_string_pretty(&cassette).unwrap();
        // Verify body field is not present when None
        assert!(!s.contains("\"body\": null"));
        assert!(!s.contains("\"body\":null"));

        // Verify it can be parsed back
        let parsed: Cassette = serde_json::from_str(&s).unwrap();
        assert_eq!(cassette, parsed);
    }

    #[test]
    fn cassette_with_multiple_interactions() {
        let cassette = Cassette {
            interactions: vec![
                Interaction {
                    request: RecordedRequest {
                        method: "POST".into(),
                        path: "/v1/messages".into(),
                        body: Some(serde_json::json!({"turn": 1})),
                    },
                    response: RecordedResponse {
                        status: 200,
                        body: serde_json::json!({"response": 1}),
                    },
                },
                Interaction {
                    request: RecordedRequest {
                        method: "POST".into(),
                        path: "/v1/messages".into(),
                        body: Some(serde_json::json!({"turn": 2})),
                    },
                    response: RecordedResponse {
                        status: 200,
                        body: serde_json::json!({"response": 2}),
                    },
                },
            ],
        };

        let s = serde_json::to_string(&cassette).unwrap();
        let parsed: Cassette = serde_json::from_str(&s).unwrap();
        assert_eq!(cassette, parsed);
        assert_eq!(cassette.interactions.len(), 2);
    }

    #[test]
    fn default_snapshot_dir_is_valid() {
        let dir = default_snapshot_dir();
        assert!(dir.ends_with("tests/snapshots"));
    }

    #[test]
    fn cassette_path_formats_correctly() {
        let dir = PathBuf::from("/tmp/snapshots");
        let path = cassette_path(&dir, "multi_turn_tool_conversation");
        assert_eq!(
            path,
            PathBuf::from("/tmp/snapshots/multi_turn_tool_conversation.json")
        );
    }
}
