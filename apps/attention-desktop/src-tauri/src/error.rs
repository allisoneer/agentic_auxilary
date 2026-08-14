use attention_client::ClientError;
use attention_protocol::ResourceRef;
use attention_protocol::V1Error;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopErrorDto {
    pub category: &'static str,
    pub message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_revision: Option<String>,
}

impl std::fmt::Display for DesktopErrorDto {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.category, self.message)
    }
}
impl std::error::Error for DesktopErrorDto {}

impl DesktopErrorDto {
    const fn simple(category: &'static str, message: &'static str) -> Self {
        Self {
            category,
            message,
            resource_kind: None,
            resource_id: None,
            expected_revision: None,
            actual_revision: None,
        }
    }
    pub const fn invalid_ack() -> Self {
        Self::simple(
            "invalid_cursor_acknowledgement",
            "acknowledgement is stale or was not delivered",
        )
    }
    pub const fn closed() -> Self {
        Self::simple("closed", "desktop connection is closed")
    }
    pub const fn validation() -> Self {
        Self::simple("validation", "input is invalid")
    }
}

fn resource(resource: ResourceRef) -> Option<(&'static str, String)> {
    match resource {
        ResourceRef::WorkItem { id } => Some(("work_item", id.0)),
        ResourceRef::AttentionSignal { id } => Some(("attention_signal", id.0)),
        ResourceRef::Reminder { id } => Some(("reminder", id.0)),
        ResourceRef::ReminderFire { id } => Some(("reminder_fire", id.0)),
        _ => None,
    }
}

impl From<ClientError> for DesktopErrorDto {
    fn from(value: ClientError) -> Self {
        match value {
            ClientError::Configuration(_) => {
                Self::simple("configuration", "desktop connection is misconfigured")
            }
            ClientError::Transport(_) | ClientError::LocalProtocol(_) => {
                Self::simple("transport", "desktop connection failed")
            }
            ClientError::Peer(error) => match V1Error::try_from(error) {
                Ok(V1Error::ExpectedRevisionConflict(data)) => {
                    let Some((kind, id)) = resource(data.resource) else {
                        return Self::simple("peer", "server rejected the request");
                    };
                    Self {
                        category: "expected_revision_conflict",
                        message: "resource revision changed",
                        resource_kind: Some(kind),
                        resource_id: Some(id),
                        expected_revision: Some(data.expected.as_str().to_owned()),
                        actual_revision: Some(data.actual.as_str().to_owned()),
                    }
                }
                Ok(V1Error::CreateConflict(data)) => {
                    let Some((kind, id)) = resource(data.resource) else {
                        return Self::simple("peer", "server rejected the request");
                    };
                    Self {
                        category: "create_conflict",
                        message: "resource already exists",
                        resource_kind: Some(kind),
                        resource_id: Some(id),
                        expected_revision: None,
                        actual_revision: None,
                    }
                }
                _ => Self::simple("peer", "server rejected the request"),
            },
            ClientError::Timeout => Self::simple("timeout", "desktop connection timed out"),
            ClientError::Backpressure(_) => {
                Self::simple("backpressure", "desktop is temporarily overloaded")
            }
            ClientError::AmbiguousMutation => {
                Self::simple("ambiguous_mutation", "mutation outcome is unknown")
            }
            ClientError::InvalidCursorAcknowledgement(_) => Self::invalid_ack(),
            ClientError::Closed => Self::closed(),
        }
    }
}
