//! Hello negotiation and subscription DTOs.

use crate::BootId;
use crate::Cursor;
use crate::ProtocolVersion;
use crate::ServerId;
use crate::StreamId;
use serde::Deserialize;
use serde::Serialize;

/// The mandatory first RPC method for protocol negotiation.
pub const RPC_HELLO_METHOD: &str = "rpc.hello";

/// Optional domain-neutral client identity information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientIdentity {
    /// Client implementation name.
    pub name: String,
    /// Optional client implementation version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Parameters for `rpc.hello`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloRequest {
    /// Requested protocol version.
    pub protocol_version: ProtocolVersion,
    /// Requested subscription behavior.
    pub subscription: SubscriptionRequest,
    /// Optional client implementation identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientIdentity>,
}

/// Requested post-hello subscription behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum SubscriptionRequest {
    /// Do not establish a subscription.
    None,
    /// Request a fresh snapshot.
    Snapshot,
    /// Resume a known stream after a cursor.
    Resume {
        /// Expected server identity.
        server_id: ServerId,
        /// Expected logical stream identity.
        stream_id: StreamId,
        /// Last cursor fully processed by the client.
        after_cursor: Cursor,
    },
}

/// Negotiated connection limits reported by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloLimits {
    /// Maximum accepted wire-message size in bytes.
    pub max_message_bytes: u32,
    /// Maximum negotiated number of in-flight requests.
    pub max_in_flight: u32,
}

/// Result of successful `rpc.hello` negotiation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloResult<S> {
    /// Selected protocol version.
    pub protocol_version: ProtocolVersion,
    /// Server identity.
    pub server_id: ServerId,
    /// Diagnostic process-boot identity.
    pub boot_id: BootId,
    /// Logical stream identity.
    pub stream_id: StreamId,
    /// Negotiated subscription outcome.
    pub subscription_result: SubscriptionResult<S>,
    /// Informational negotiated limits.
    pub limits: HelloLimits,
}

/// Negotiated post-hello subscription outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum SubscriptionResult<S> {
    /// No subscription was established.
    None,
    /// A fresh generic snapshot was returned.
    Snapshot {
        /// Domain state supplied by a later protocol layer.
        state: S,
        /// Cursor immediately after the snapshot.
        after_cursor: Cursor,
    },
    /// A known stream was resumed.
    Resume {
        /// Cursor after which replay or live delivery begins.
        after_cursor: Cursor,
    },
}

/// Concrete v1 hello result for the Attention state model.
pub type AttentionHelloResult = HelloResult<crate::AttentionSnapshot>;

#[cfg(test)]
mod tests {
    use super::ClientIdentity;
    use super::HelloLimits;
    use super::HelloRequest;
    use super::HelloResult;
    use super::SubscriptionRequest;
    use super::SubscriptionResult;
    use crate::BootId;
    use crate::Cursor;
    use crate::PROTOCOL_V1;
    use crate::ServerId;
    use crate::StreamId;
    use serde_json::Value;
    use serde_json::json;

    #[test]
    fn request_modes_have_exact_wire_names_and_optional_client_fields() {
        let none = HelloRequest {
            protocol_version: PROTOCOL_V1,
            subscription: SubscriptionRequest::None,
            client: None,
        };
        let none_value = serde_json::to_value(none).expect("serialize none request");
        assert_eq!(none_value["subscription"]["mode"], "none");
        assert!(none_value.get("client").is_none());

        let snapshot = HelloRequest {
            protocol_version: PROTOCOL_V1,
            subscription: SubscriptionRequest::Snapshot,
            client: Some(ClientIdentity {
                name: "attention-cli".to_string(),
                version: None,
            }),
        };
        let snapshot_value = serde_json::to_value(snapshot).expect("serialize snapshot request");
        assert_eq!(snapshot_value["subscription"]["mode"], "snapshot");
        assert!(snapshot_value["client"].get("version").is_none());

        let resume = HelloRequest {
            protocol_version: PROTOCOL_V1,
            subscription: SubscriptionRequest::Resume {
                server_id: ServerId("server-1".to_string()),
                stream_id: StreamId("stream-1".to_string()),
                after_cursor: Cursor("cursor-1".to_string()),
            },
            client: Some(ClientIdentity {
                name: "attention-cli".to_string(),
                version: Some("1.2.3".to_string()),
            }),
        };
        let resume_value = serde_json::to_value(resume).expect("serialize resume request");
        assert_eq!(resume_value["subscription"]["mode"], "resume");
        assert_eq!(resume_value["subscription"]["server_id"], "server-1");
        assert_eq!(resume_value["subscription"]["stream_id"], "stream-1");
        assert_eq!(resume_value["subscription"]["after_cursor"], "cursor-1");
        assert_eq!(resume_value["client"]["version"], "1.2.3");
    }

    #[test]
    fn result_modes_keep_generic_state_and_nested_limits() {
        let snapshot = HelloResult {
            protocol_version: PROTOCOL_V1,
            server_id: ServerId("server-1".to_string()),
            boot_id: BootId("boot-1".to_string()),
            stream_id: StreamId("stream-1".to_string()),
            subscription_result: SubscriptionResult::Snapshot {
                state: json!({}),
                after_cursor: Cursor("cursor-1".to_string()),
            },
            limits: HelloLimits {
                max_message_bytes: 1_048_576,
                max_in_flight: 32,
            },
        };
        let value = serde_json::to_value(snapshot).expect("serialize snapshot result");
        assert_eq!(value["subscription_result"]["mode"], "snapshot");
        assert_eq!(value["subscription_result"]["state"], json!({}));
        assert_eq!(value["limits"]["max_message_bytes"], 1_048_576);
        assert_eq!(value["limits"]["max_in_flight"], 32);

        let none: SubscriptionResult<Value> = SubscriptionResult::None;
        assert_eq!(
            serde_json::to_value(none).expect("serialize none result")["mode"],
            "none"
        );
        let resume: SubscriptionResult<Value> = SubscriptionResult::Resume {
            after_cursor: Cursor("cursor-2".to_string()),
        };
        assert_eq!(
            serde_json::to_value(resume).expect("serialize resume result"),
            json!({"mode": "resume", "after_cursor": "cursor-2"})
        );
    }

    #[test]
    fn unknown_modes_are_rejected_but_additive_fields_are_accepted() {
        assert!(serde_json::from_value::<SubscriptionRequest>(json!({"mode": "future"})).is_err());
        assert!(
            serde_json::from_value::<SubscriptionResult<Value>>(json!({"mode": "future"})).is_err()
        );
        assert_eq!(
            serde_json::from_value::<SubscriptionRequest>(json!({
                "mode": "none",
                "future": true
            }))
            .expect("additive request field"),
            SubscriptionRequest::None
        );
    }
}
