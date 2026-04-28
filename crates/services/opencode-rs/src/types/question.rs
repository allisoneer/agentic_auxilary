//! Question types for `opencode_rs`.
//!
//! Types for the question-answer flow where the server asks users for input.

use serde::Deserialize;
use serde::Serialize;

/// A question request from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionRequest {
    /// Unique request identifier.
    pub id: String,

    /// Session ID.
    #[serde(rename = "sessionID")]
    pub session_id: String,

    /// List of questions to present.
    pub questions: Vec<QuestionInfo>,

    /// Tool context if this question is from a tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<QuestionToolContext>,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// A single question with options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionInfo {
    /// The question text.
    pub question: String,

    /// Header to display above the question.
    #[serde(default)]
    pub header: String,

    /// Available options for the answer.
    #[serde(default)]
    pub options: Vec<QuestionOption>,

    /// Whether multiple options can be selected.
    #[serde(default)]
    pub multiple: bool,

    /// Whether custom input is allowed.
    #[serde(default = "default_true")]
    pub custom: bool,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// An option for a question.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    /// Option label.
    pub label: String,

    /// Option description.
    #[serde(default)]
    pub description: String,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Tool context for a question.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionToolContext {
    /// Message ID containing the tool call.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "messageID")]
    pub message_id: Option<String>,

    /// Tool call ID.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "callID")]
    pub call_id: Option<String>,

    /// Tool name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Reply to a question request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionReply {
    /// Answers for each question (list of selected option labels/values).
    pub answers: Vec<Vec<String>>,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_question_request_minimal() {
        let json = r#"{
            "id": "req-123",
            "sessionID": "sess-456",
            "questions": []
        }"#;
        let req: QuestionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "req-123");
        assert_eq!(req.session_id, "sess-456");
        assert!(req.questions.is_empty());
        assert!(req.tool.is_none());
    }

    #[test]
    fn test_question_request_full() {
        let json = r#"{
            "id": "req-123",
            "sessionID": "sess-456",
            "questions": [
                {
                    "question": "What do you want to do?",
                    "header": "Choose an action",
                    "options": [
                        {"label": "Save", "description": "Save the file"},
                        {"label": "Discard", "description": "Discard changes"}
                    ],
                    "multiple": false,
                    "custom": true
                }
            ],
            "tool": {
                "messageID": "msg-1",
                "callID": "call-1",
                "name": "confirm"
            }
        }"#;
        let req: QuestionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "req-123");
        assert_eq!(req.questions.len(), 1);

        let q = &req.questions[0];
        assert_eq!(q.question, "What do you want to do?");
        assert_eq!(q.header, "Choose an action");
        assert_eq!(q.options.len(), 2);
        assert!(!q.multiple);
        assert!(q.custom);

        let tool = req.tool.unwrap();
        assert_eq!(tool.message_id, Some("msg-1".to_string()));
        assert_eq!(tool.name, Some("confirm".to_string()));
    }

    #[test]
    fn test_question_info_defaults() {
        let json = r#"{"question": "Continue?"}"#;
        let info: QuestionInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.question, "Continue?");
        assert!(info.header.is_empty());
        assert!(info.options.is_empty());
        assert!(!info.multiple);
        assert!(info.custom); // defaults to true
    }

    #[test]
    fn test_question_option() {
        let json = r#"{"label": "Yes", "description": "Confirm action"}"#;
        let opt: QuestionOption = serde_json::from_str(json).unwrap();
        assert_eq!(opt.label, "Yes");
        assert_eq!(opt.description, "Confirm action");
    }

    #[test]
    fn test_question_reply() {
        let reply = QuestionReply {
            answers: vec![vec!["Save".to_string()]],
        };
        let json = serde_json::to_string(&reply).unwrap();
        assert!(json.contains("Save"));

        let parsed: QuestionReply = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.answers.len(), 1);
        assert_eq!(parsed.answers[0], vec!["Save"]);
    }

    #[test]
    fn test_question_reply_multiple() {
        let json = r#"{"answers": [["A", "B"], ["C"]]}"#;
        let reply: QuestionReply = serde_json::from_str(json).unwrap();
        assert_eq!(reply.answers.len(), 2);
        assert_eq!(reply.answers[0], vec!["A", "B"]);
        assert_eq!(reply.answers[1], vec!["C"]);
    }

    #[test]
    fn test_question_extra_fields_preserved() {
        let json = r#"{
            "id": "req-123",
            "sessionID": "sess-456",
            "questions": [],
            "futureField": "value"
        }"#;
        let req: QuestionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.extra["futureField"], "value");
    }
}
