use super::location::LocationInfo;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    pub data: serde_json::Value,
}

impl Event {
    pub fn session_id(&self) -> Option<&str> {
        self.data
            .get("sessionID")
            .or_else(|| self.data.get("sessionId"))
            .and_then(serde_json::Value::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_event_wrapper_shape() {
        let event: Event = serde_json::from_str(
            r#"{
                "id": "evt-1",
                "type": "question.asked",
                "location": {"directory": "/tmp/project"},
                "version": 2,
                "data": {"sessionID": "session-1", "payload": "ok"}
            }"#,
        )
        .unwrap();

        assert_eq!(event.event_type, "question.asked");
        assert_eq!(
            event
                .location
                .as_ref()
                .map(|location| location.directory.as_str()),
            Some("/tmp/project")
        );
        assert_eq!(event.session_id(), Some("session-1"));
    }
}
