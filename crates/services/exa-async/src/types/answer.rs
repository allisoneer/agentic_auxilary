//! Types for the Exa `/answer` endpoint (non-streaming)

use serde::{Deserialize, Serialize};

/// Request body for `POST /answer`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerRequest {
    /// The query to answer
    pub query: String,

    /// Model to use (e.g., "exa" or "exa-pro")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// System prompt to guide the answer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Text to display when streaming (non-streaming only uses query)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<bool>,
}

impl AnswerRequest {
    /// Create a new answer request with the given query
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            model: None,
            system_prompt: None,
            text: None,
        }
    }

    /// Set the model
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

/// A citation in an answer response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    /// URL of the cited source
    pub url: String,
    /// Title of the cited source
    #[serde(default)]
    pub title: Option<String>,
    /// ID of the cited source
    #[serde(default)]
    pub id: Option<String>,
    /// Published date
    #[serde(default)]
    pub published_date: Option<String>,
    /// Author
    #[serde(default)]
    pub author: Option<String>,
}

/// Response from `POST /answer` (non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerResponse {
    /// The generated answer
    pub answer: String,

    /// Citations used in the answer
    #[serde(default)]
    pub citations: Vec<Citation>,

    /// Cost in dollars
    #[serde(default)]
    pub cost_dollars: Option<super::search::CostDollars>,
}
