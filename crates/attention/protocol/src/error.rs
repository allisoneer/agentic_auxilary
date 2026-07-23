//! Wire and local protocol error types.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use thiserror::Error;

/// An open JSON-RPC error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ErrorCode(pub i32);

/// Invalid JSON was received.
pub const PARSE_ERROR: ErrorCode = ErrorCode(-32700);
/// The JSON-RPC request object was invalid.
pub const INVALID_REQUEST: ErrorCode = ErrorCode(-32600);
/// The requested method does not exist.
pub const METHOD_NOT_FOUND: ErrorCode = ErrorCode(-32601);
/// The method parameters were invalid.
pub const INVALID_PARAMS: ErrorCode = ErrorCode(-32602);
/// An internal JSON-RPC error occurred.
pub const INTERNAL_ERROR: ErrorCode = ErrorCode(-32603);
/// The requested Attention protocol version is unsupported.
pub const UNSUPPORTED_PROTOCOL_VERSION: ErrorCode = ErrorCode(-32090);
/// The peer attempted another operation before `rpc.hello`.
pub const HELLO_REQUIRED: ErrorCode = ErrorCode(-32091);

/// A peer-visible JSON-RPC error object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    /// Machine-readable error code.
    pub code: ErrorCode,
    /// Sanitized peer-visible message.
    pub message: String,
    /// Optional peer-visible structured details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    /// Builds a sanitized unsupported-version error for peer-visible negotiation.
    pub fn unsupported_protocol_version(supported_versions: &[crate::ProtocolVersion]) -> Self {
        Self {
            code: UNSUPPORTED_PROTOCOL_VERSION,
            message: "Unsupported protocol version".to_string(),
            data: Some(json!({ "supported_versions": supported_versions })),
        }
    }
}

/// Local protocol validation failures that are never serialized directly.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// Protocol-version validation failed.
    #[error("protocol version validation failed: {0}")]
    VersionValidation(String),
    /// Parameter-shape validation failed.
    #[error("parameter validation failed: {0}")]
    ParamsValidation(String),
    /// Envelope-shape validation failed.
    #[error("envelope validation failed: {0}")]
    EnvelopeValidation(String),
    /// Response-shape validation failed.
    #[error("response validation failed: {0}")]
    ResponseValidation(String),
    /// Identifier validation failed.
    #[error("identifier validation failed: {0}")]
    IdValidation(String),
    /// Timestamp validation failed.
    #[error("timestamp validation failed: {0}")]
    TimestampValidation(String),
}

#[cfg(test)]
mod tests {
    use super::ErrorCode;
    use super::HELLO_REQUIRED;
    use super::INTERNAL_ERROR;
    use super::INVALID_PARAMS;
    use super::INVALID_REQUEST;
    use super::METHOD_NOT_FOUND;
    use super::PARSE_ERROR;
    use super::RpcError;
    use super::UNSUPPORTED_PROTOCOL_VERSION;

    #[test]
    fn named_error_codes_are_frozen() {
        assert_eq!(PARSE_ERROR, ErrorCode(-32700));
        assert_eq!(INVALID_REQUEST, ErrorCode(-32600));
        assert_eq!(METHOD_NOT_FOUND, ErrorCode(-32601));
        assert_eq!(INVALID_PARAMS, ErrorCode(-32602));
        assert_eq!(INTERNAL_ERROR, ErrorCode(-32603));
        assert_eq!(UNSUPPORTED_PROTOCOL_VERSION, ErrorCode(-32090));
        assert_eq!(HELLO_REQUIRED, ErrorCode(-32091));
    }

    #[test]
    fn unknown_error_codes_round_trip_and_absent_data_is_omitted() {
        let error = RpcError {
            code: ErrorCode(-32123),
            message: "future error".to_string(),
            data: None,
        };
        let value = serde_json::to_value(&error).expect("serialize error");
        assert_eq!(value["code"], -32123);
        assert!(value.get("data").is_none());
        assert_eq!(
            serde_json::from_value::<RpcError>(value).expect("deserialize error"),
            error
        );
    }

    #[test]
    fn unsupported_version_error_exposes_only_supported_versions() {
        let error = RpcError::unsupported_protocol_version(&[crate::PROTOCOL_V1]);
        let value = serde_json::to_value(error).expect("serialize unsupported-version error");
        assert_eq!(value["code"], -32090);
        assert_eq!(value["message"], "Unsupported protocol version");
        assert_eq!(
            value["data"],
            serde_json::json!({"supported_versions": [1]})
        );
    }
}
