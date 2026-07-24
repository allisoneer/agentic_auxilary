//! Wire and local protocol error types.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use thiserror::Error;

use crate::AttentionSignalId;
use crate::CursorGap;
use crate::MutationIdempotencyKey;
use crate::OccurrenceKey;
use crate::OutboxIntentId;
use crate::ReminderFireId;
use crate::ReminderId;
use crate::Revision;
use crate::SourceEntityKey;
use crate::SourceReceiptId;
use crate::SourceStateVersion;
use crate::WorkItemId;

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
/// A requested public resource does not exist.
pub const RESOURCE_NOT_FOUND: ErrorCode = ErrorCode(-32080);
/// An optimistic resource revision did not match.
pub const EXPECTED_REVISION_CONFLICT: ErrorCode = ErrorCode(-32081);
/// A create operation collided with existing authority.
pub const CREATE_CONFLICT: ErrorCode = ErrorCode(-32082);
/// An idempotency key was reused with different semantic input.
pub const IDEMPOTENCY_MISMATCH: ErrorCode = ErrorCode(-32083);
/// A source occurrence identity was reused with different content.
pub const OCCURRENCE_CONTENT_MISMATCH: ErrorCode = ErrorCode(-32084);
/// Observed source authority no longer matches current authority.
pub const SOURCE_VERSION_CONFLICT: ErrorCode = ErrorCode(-32085);
/// A requested cursor cannot be replayed and requires a snapshot.
pub const CURSOR_GAP: ErrorCode = ErrorCode(-32086);
/// No delivery authority exists for the requested intent.
pub const DELIVERY_NOT_FOUND: ErrorCode = ErrorCode(-32087);

/// Exact v1 domain error codes, used by conformance governance.
pub const V1_ERROR_CODES: &[ErrorCode] = &[
    RESOURCE_NOT_FOUND,
    EXPECTED_REVISION_CONFLICT,
    CREATE_CONFLICT,
    IDEMPOTENCY_MISMATCH,
    OCCURRENCE_CONTENT_MISMATCH,
    SOURCE_VERSION_CONFLICT,
    CURSOR_GAP,
    DELIVERY_NOT_FOUND,
];

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

/// Typed reference to a public protocol resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourceRef {
    WorkItem { id: WorkItemId },
    AttentionSignal { id: AttentionSignalId },
    Reminder { id: ReminderId },
    ReminderFire { id: ReminderFireId },
    SourceReceipt { id: SourceReceiptId },
    SourceEntity { key: SourceEntityKey },
    SourceOccurrence { key: OccurrenceKey },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceNotFoundData {
    pub resource: ResourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedRevisionConflictData {
    pub resource: ResourceRef,
    pub expected: Revision,
    pub actual: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateConflictData {
    pub resource: ResourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyMismatchData {
    pub idempotency_key: MutationIdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccurrenceContentMismatchData {
    pub occurrence_key: OccurrenceKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceVersionConflictData {
    pub entity: SourceEntityKey,
    pub observed: Option<SourceStateVersion>,
    pub actual: Option<SourceStateVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryNotFoundData {
    pub intent_id: OutboxIntentId,
}

/// Typed v1 error surface with an open unknown-code representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V1Error {
    ResourceNotFound(ResourceNotFoundData),
    ExpectedRevisionConflict(ExpectedRevisionConflictData),
    CreateConflict(CreateConflictData),
    IdempotencyMismatch(IdempotencyMismatchData),
    OccurrenceContentMismatch(OccurrenceContentMismatchData),
    SourceVersionConflict(SourceVersionConflictData),
    CursorGap(CursorGap),
    DeliveryNotFound(DeliveryNotFoundData),
    Unknown(RpcError),
}

/// A known v1 code had a noncanonical message or malformed data.
#[derive(Debug, Error)]
#[error("malformed known v1 error {code:?}: {reason}")]
pub struct V1ErrorDecodeError {
    pub code: ErrorCode,
    pub reason: String,
}

impl TryFrom<RpcError> for V1Error {
    type Error = V1ErrorDecodeError;

    fn try_from(error: RpcError) -> Result<Self, Self::Error> {
        macro_rules! known {
            ($message:literal, $data_type:ty, $variant:ident) => {{
                require_message(&error, $message)?;
                let data = require_data::<$data_type>(&error)?;
                Ok(Self::$variant(data))
            }};
        }

        match error.code {
            RESOURCE_NOT_FOUND => {
                known!("Resource not found", ResourceNotFoundData, ResourceNotFound)
            }
            EXPECTED_REVISION_CONFLICT => known!(
                "Expected revision conflict",
                ExpectedRevisionConflictData,
                ExpectedRevisionConflict
            ),
            CREATE_CONFLICT => known!("Create conflict", CreateConflictData, CreateConflict),
            IDEMPOTENCY_MISMATCH => known!(
                "Idempotency mismatch",
                IdempotencyMismatchData,
                IdempotencyMismatch
            ),
            OCCURRENCE_CONTENT_MISMATCH => known!(
                "Occurrence content mismatch",
                OccurrenceContentMismatchData,
                OccurrenceContentMismatch
            ),
            SOURCE_VERSION_CONFLICT => known!(
                "Source version conflict",
                SourceVersionConflictData,
                SourceVersionConflict
            ),
            CURSOR_GAP => known!("Cursor gap", CursorGap, CursorGap),
            DELIVERY_NOT_FOUND => {
                known!("Delivery not found", DeliveryNotFoundData, DeliveryNotFound)
            }
            _ => Ok(Self::Unknown(error)),
        }
    }
}

impl TryFrom<V1Error> for RpcError {
    type Error = serde_json::Error;

    fn try_from(error: V1Error) -> Result<Self, Self::Error> {
        macro_rules! known {
            ($code:ident, $message:literal, $data:expr) => {
                Ok(Self {
                    code: $code,
                    message: $message.to_string(),
                    data: Some(serde_json::to_value($data)?),
                })
            };
        }

        match error {
            V1Error::ResourceNotFound(data) => {
                known!(RESOURCE_NOT_FOUND, "Resource not found", data)
            }
            V1Error::ExpectedRevisionConflict(data) => known!(
                EXPECTED_REVISION_CONFLICT,
                "Expected revision conflict",
                data
            ),
            V1Error::CreateConflict(data) => known!(CREATE_CONFLICT, "Create conflict", data),
            V1Error::IdempotencyMismatch(data) => {
                known!(IDEMPOTENCY_MISMATCH, "Idempotency mismatch", data)
            }
            V1Error::OccurrenceContentMismatch(data) => known!(
                OCCURRENCE_CONTENT_MISMATCH,
                "Occurrence content mismatch",
                data
            ),
            V1Error::SourceVersionConflict(data) => {
                known!(SOURCE_VERSION_CONFLICT, "Source version conflict", data)
            }
            V1Error::CursorGap(data) => known!(CURSOR_GAP, "Cursor gap", data),
            V1Error::DeliveryNotFound(data) => {
                known!(DELIVERY_NOT_FOUND, "Delivery not found", data)
            }
            V1Error::Unknown(error) => Ok(error),
        }
    }
}

fn require_message(error: &RpcError, expected: &'static str) -> Result<(), V1ErrorDecodeError> {
    if error.message == expected {
        Ok(())
    } else {
        Err(decode_error(error.code, "noncanonical message"))
    }
}

fn require_data<T>(error: &RpcError) -> Result<T, V1ErrorDecodeError>
where
    T: for<'de> Deserialize<'de>,
{
    let data = error
        .data
        .clone()
        .ok_or_else(|| decode_error(error.code, "missing data"))?;
    serde_json::from_value(data).map_err(|decode| decode_error(error.code, decode.to_string()))
}

fn decode_error(code: ErrorCode, reason: impl Into<String>) -> V1ErrorDecodeError {
    V1ErrorDecodeError {
        code,
        reason: reason.into(),
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
    /// Canonical scalar validation failed.
    #[error("value validation failed: {0}")]
    ValueValidation(String),
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
