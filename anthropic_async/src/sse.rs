//! Server-Sent Events (SSE) streaming support.
//!
//! This module provides streaming response handling for the Messages API.

#[cfg(feature = "streaming")]
/// Streaming API implementation
pub mod streaming {
    use futures::Stream;
    use serde::{Deserialize, Serialize};
    use std::pin::Pin;

    use crate::error::AnthropicError;
    use crate::types::content::{ContentBlock, MessageRole};
    use crate::types::messages::MessagesCreateResponse;

    /// Type alias for the event stream returned by streaming APIs
    pub type EventStream =
        Pin<Box<dyn Stream<Item = Result<Event, AnthropicError>> + Send + 'static>>;

    // =========================================================================
    // SSE Frame and Decoder
    // =========================================================================

    /// Raw SSE frame with optional event type and data payload
    #[derive(Debug, Clone, Default)]
    pub struct SseFrame {
        /// Event type (from `event:` line)
        pub event: Option<String>,
        /// Data payload (from `data:` lines, may be multiline)
        pub data: String,
    }

    /// SSE decoder that parses raw bytes into frames
    ///
    /// Handles:
    /// - Multi-line data (multiple `data:` lines)
    /// - Chunk boundaries splitting lines
    /// - Empty `data:` lines
    /// - Unknown fields (ignored per SSE spec)
    #[derive(Debug, Default)]
    pub struct SSEDecoder {
        buffer: String,
        current_frame: SseFrame,
    }

    impl SSEDecoder {
        /// Create a new decoder
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Push a chunk of bytes and return any complete frames
        pub fn push(&mut self, chunk: &[u8]) -> Vec<SseFrame> {
            let text = String::from_utf8_lossy(chunk);
            self.buffer.push_str(&text);

            let mut frames = Vec::new();

            // Process complete lines
            while let Some(newline_pos) = self.buffer.find('\n') {
                let line = self.buffer[..newline_pos]
                    .trim_end_matches('\r')
                    .to_string();
                self.buffer = self.buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    // Blank line = end of frame
                    if self.current_frame.event.is_some() || !self.current_frame.data.is_empty() {
                        frames.push(std::mem::take(&mut self.current_frame));
                    }
                } else if let Some(value) = line.strip_prefix("event:") {
                    self.current_frame.event = Some(value.trim().to_string());
                } else if let Some(value) = line.strip_prefix("data:") {
                    let data_value = value.strip_prefix(' ').unwrap_or(value);
                    if !self.current_frame.data.is_empty() {
                        self.current_frame.data.push('\n');
                    }
                    self.current_frame.data.push_str(data_value);
                }
                // Ignore other fields (id:, retry:, comments starting with :)
            }

            frames
        }

        /// Flush any remaining data as a final frame
        ///
        /// This processes any incomplete line still in the buffer before returning
        /// the current frame.
        pub fn flush(&mut self) -> Option<SseFrame> {
            // Process any remaining incomplete line in the buffer
            if !self.buffer.is_empty() {
                let line = std::mem::take(&mut self.buffer);
                let line = line.trim_end_matches('\r');
                if let Some(value) = line.strip_prefix("event:") {
                    self.current_frame.event = Some(value.trim().to_string());
                } else if let Some(value) = line.strip_prefix("data:") {
                    let data_value = value.strip_prefix(' ').unwrap_or(value);
                    if !self.current_frame.data.is_empty() {
                        self.current_frame.data.push('\n');
                    }
                    self.current_frame.data.push_str(data_value);
                }
            }

            if self.current_frame.event.is_some() || !self.current_frame.data.is_empty() {
                Some(std::mem::take(&mut self.current_frame))
            } else {
                None
            }
        }
    }

    // =========================================================================
    // Typed Event Structures
    // =========================================================================

    /// Streaming event types from Anthropic Messages API
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    #[allow(clippy::derive_partial_eq_without_eq)] // ContentBlock doesn't impl Eq
    pub enum Event {
        /// Message creation started
        MessageStart {
            /// The message object being built
            message: MessageStartPayload,
        },
        /// Content block started
        ContentBlockStart {
            /// Index of the content block
            index: usize,
            /// Initial content block data
            content_block: ContentBlockStartData,
        },
        /// Delta update for a content block
        ContentBlockDelta {
            /// Index of the content block being updated
            index: usize,
            /// The delta data
            delta: ContentBlockDeltaData,
        },
        /// Content block completed
        ContentBlockStop {
            /// Index of the completed content block
            index: usize,
        },
        /// Message metadata delta (usage, `stop_reason`)
        MessageDelta {
            /// Delta containing `stop_reason` and usage
            delta: MessageDeltaPayload,
            /// Updated usage information
            #[serde(skip_serializing_if = "Option::is_none")]
            usage: Option<MessageDeltaUsage>,
        },
        /// Message streaming completed
        MessageStop,
        /// Ping event (keep-alive)
        Ping,
        /// Error event
        Error {
            /// Error details
            error: EventError,
        },
    }

    /// Payload for `message_start` event
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct MessageStartPayload {
        /// Message ID
        pub id: String,
        /// Type (always "message")
        #[serde(rename = "type")]
        pub kind: String,
        /// Role (always "assistant")
        pub role: MessageRole,
        /// Model used
        pub model: String,
        /// Initial content (usually empty)
        #[serde(default)]
        pub content: Vec<ContentBlock>,
        /// Stop reason (None initially)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub stop_reason: Option<String>,
        /// Stop sequence (None initially)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub stop_sequence: Option<String>,
        /// Initial usage
        #[serde(skip_serializing_if = "Option::is_none")]
        pub usage: Option<MessageStartUsage>,
    }

    /// Usage information in `message_start`
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct MessageStartUsage {
        /// Input tokens
        pub input_tokens: u64,
        /// Output tokens (initially 0)
        pub output_tokens: u64,
        /// Cache creation input tokens
        #[serde(skip_serializing_if = "Option::is_none")]
        pub cache_creation_input_tokens: Option<u64>,
        /// Cache read input tokens
        #[serde(skip_serializing_if = "Option::is_none")]
        pub cache_read_input_tokens: Option<u64>,
    }

    /// Content block type at start
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum ContentBlockStartData {
        /// Text block
        Text {
            /// Initial text (usually empty string)
            text: String,
        },
        /// Tool use block
        ToolUse {
            /// Tool use ID
            id: String,
            /// Tool name
            name: String,
            /// Initial input (usually empty object)
            input: serde_json::Value,
        },
    }

    /// Content block delta data
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum ContentBlockDeltaData {
        /// Text delta
        TextDelta {
            /// Text to append
            text: String,
        },
        /// JSON delta for tool input
        InputJsonDelta {
            /// Partial JSON to append
            partial_json: String,
        },
        /// Thinking delta (forward-compatible, extended thinking feature)
        ThinkingDelta {
            /// Thinking text to append
            thinking: String,
        },
        /// Citations delta (forward-compatible, web search feature)
        CitationsDelta {
            /// Partial citations JSON to append
            citation: String,
        },
        /// Signature delta (forward-compatible)
        SignatureDelta {
            /// Signature to append
            signature: String,
        },
        /// Catch-all for unknown/future delta types
        #[serde(other)]
        Unknown,
    }

    /// Message delta payload
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
    pub struct MessageDeltaPayload {
        /// Stop reason
        #[serde(skip_serializing_if = "Option::is_none")]
        pub stop_reason: Option<String>,
        /// Stop sequence that triggered stop
        #[serde(skip_serializing_if = "Option::is_none")]
        pub stop_sequence: Option<String>,
    }

    /// Usage information in `message_delta`
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct MessageDeltaUsage {
        /// Output tokens generated so far
        pub output_tokens: u64,
    }

    /// Error details in error event
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct EventError {
        /// Error type
        #[serde(rename = "type")]
        pub kind: String,
        /// Error message
        pub message: String,
    }

    // =========================================================================
    // Event Parsing
    // =========================================================================

    impl Event {
        /// Parse an SSE frame into a typed Event
        ///
        /// # Errors
        ///
        /// Returns an error if the event type is unrecognized or the data cannot be parsed.
        pub fn from_frame(frame: &SseFrame) -> Result<Self, AnthropicError> {
            let event_type = frame.event.as_deref().unwrap_or("message");

            match event_type {
                "message_start" => {
                    let payload: MessageStartEvent = serde_json::from_str(&frame.data)
                        .map_err(|e| AnthropicError::Serde(format!("message_start: {e}")))?;
                    Ok(Self::MessageStart {
                        message: payload.message,
                    })
                }
                "content_block_start" => {
                    let payload: ContentBlockStartEvent = serde_json::from_str(&frame.data)
                        .map_err(|e| AnthropicError::Serde(format!("content_block_start: {e}")))?;
                    Ok(Self::ContentBlockStart {
                        index: payload.index,
                        content_block: payload.content_block,
                    })
                }
                "content_block_delta" => {
                    let payload: ContentBlockDeltaEvent = serde_json::from_str(&frame.data)
                        .map_err(|e| AnthropicError::Serde(format!("content_block_delta: {e}")))?;
                    Ok(Self::ContentBlockDelta {
                        index: payload.index,
                        delta: payload.delta,
                    })
                }
                "content_block_stop" => {
                    let payload: ContentBlockStopEvent = serde_json::from_str(&frame.data)
                        .map_err(|e| AnthropicError::Serde(format!("content_block_stop: {e}")))?;
                    Ok(Self::ContentBlockStop {
                        index: payload.index,
                    })
                }
                "message_delta" => {
                    let payload: MessageDeltaEvent = serde_json::from_str(&frame.data)
                        .map_err(|e| AnthropicError::Serde(format!("message_delta: {e}")))?;
                    Ok(Self::MessageDelta {
                        delta: payload.delta,
                        usage: payload.usage,
                    })
                }
                "message_stop" => Ok(Self::MessageStop),
                "ping" => Ok(Self::Ping),
                "error" => {
                    let payload: ErrorEvent = serde_json::from_str(&frame.data)
                        .map_err(|e| AnthropicError::Serde(format!("error event: {e}")))?;
                    Ok(Self::Error {
                        error: payload.error,
                    })
                }
                _ => {
                    // Unknown event type, try to parse as generic message or skip
                    Err(AnthropicError::Serde(format!(
                        "Unknown event type: {event_type}"
                    )))
                }
            }
        }
    }

    // Wire format structures for deserialization
    #[derive(Deserialize)]
    struct MessageStartEvent {
        message: MessageStartPayload,
    }

    #[derive(Deserialize)]
    struct ContentBlockStartEvent {
        index: usize,
        content_block: ContentBlockStartData,
    }

    #[derive(Deserialize)]
    struct ContentBlockDeltaEvent {
        index: usize,
        delta: ContentBlockDeltaData,
    }

    #[derive(Deserialize)]
    struct ContentBlockStopEvent {
        index: usize,
    }

    #[derive(Deserialize)]
    struct MessageDeltaEvent {
        delta: MessageDeltaPayload,
        usage: Option<MessageDeltaUsage>,
    }

    #[derive(Deserialize)]
    struct ErrorEvent {
        error: EventError,
    }

    // =========================================================================
    // Accumulator
    // =========================================================================

    /// Accumulates streaming events into a complete response
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut acc = Accumulator::new();
    /// while let Some(event) = stream.next().await {
    ///     if let Some(response) = acc.apply(&event?)? {
    ///         // Response is complete
    ///         return Ok(response);
    ///     }
    /// }
    /// ```
    #[derive(Debug, Default)]
    pub struct Accumulator {
        id: Option<String>,
        model: Option<String>,
        role: Option<MessageRole>,
        content_blocks: Vec<AccumulatorBlock>,
        stop_reason: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        cache_creation_input_tokens: Option<u64>,
        cache_read_input_tokens: Option<u64>,
        complete: bool,
    }

    #[derive(Debug, Clone)]
    enum AccumulatorBlock {
        Text(String),
        ToolUse {
            id: String,
            name: String,
            input_json: String,
        },
    }

    impl Accumulator {
        /// Create a new accumulator
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Apply an event to the accumulator
        ///
        /// Returns `Some(response)` when the message is complete (after `message_stop`).
        ///
        /// # Errors
        ///
        /// Returns an error if:
        /// - An error event is received
        /// - JSON parsing fails for tool inputs
        /// - Events arrive out of order
        pub fn apply(
            &mut self,
            event: &Event,
        ) -> Result<Option<MessagesCreateResponse>, AnthropicError> {
            match event {
                Event::MessageStart { message } => {
                    self.id = Some(message.id.clone());
                    self.model = Some(message.model.clone());
                    self.role = Some(message.role.clone());
                    if let Some(usage) = &message.usage {
                        self.input_tokens = Some(usage.input_tokens);
                        self.output_tokens = Some(usage.output_tokens);
                        self.cache_creation_input_tokens = usage.cache_creation_input_tokens;
                        self.cache_read_input_tokens = usage.cache_read_input_tokens;
                    }
                }
                Event::ContentBlockStart {
                    index,
                    content_block,
                } => {
                    // Ensure we have enough slots
                    while self.content_blocks.len() <= *index {
                        self.content_blocks
                            .push(AccumulatorBlock::Text(String::new()));
                    }
                    self.content_blocks[*index] = match content_block {
                        ContentBlockStartData::Text { text } => {
                            AccumulatorBlock::Text(text.clone())
                        }
                        ContentBlockStartData::ToolUse { id, name, .. } => {
                            AccumulatorBlock::ToolUse {
                                id: id.clone(),
                                name: name.clone(),
                                input_json: String::new(),
                            }
                        }
                    };
                }
                Event::ContentBlockDelta { index, delta } => {
                    if *index >= self.content_blocks.len() {
                        return Err(AnthropicError::Serde(format!(
                            "Delta for unknown block index {index}"
                        )));
                    }
                    match (&mut self.content_blocks[*index], delta) {
                        (
                            AccumulatorBlock::Text(text),
                            ContentBlockDeltaData::TextDelta { text: t },
                        ) => {
                            text.push_str(t);
                        }
                        (
                            AccumulatorBlock::ToolUse { input_json, .. },
                            ContentBlockDeltaData::InputJsonDelta { partial_json },
                        ) => {
                            input_json.push_str(partial_json);
                        }
                        // Forward-compatible: ignore mismatched or unknown delta types
                        _ => {}
                    }
                }
                Event::ContentBlockStop { .. } | Event::Ping => {
                    // Block complete or keep-alive, nothing to do
                }
                Event::MessageDelta { delta, usage } => {
                    if let Some(reason) = &delta.stop_reason {
                        self.stop_reason = Some(reason.clone());
                    }
                    if let Some(u) = usage {
                        self.output_tokens = Some(u.output_tokens);
                    }
                }
                Event::MessageStop => {
                    self.complete = true;
                }
                Event::Error { error } => {
                    return Err(AnthropicError::Api(crate::error::ApiErrorObject {
                        r#type: Some(error.kind.clone()),
                        message: error.message.clone(),
                        request_id: None,
                        code: None,
                    }));
                }
            }

            if self.complete {
                Ok(Some(self.build_response()?))
            } else {
                Ok(None)
            }
        }

        /// Build the final response from accumulated data
        fn build_response(&self) -> Result<MessagesCreateResponse, AnthropicError> {
            let content = self
                .content_blocks
                .iter()
                .map(|block| match block {
                    AccumulatorBlock::Text(text) => Ok(ContentBlock::Text { text: text.clone() }),
                    AccumulatorBlock::ToolUse {
                        id,
                        name,
                        input_json,
                    } => {
                        let input: serde_json::Value = if input_json.is_empty() {
                            serde_json::Value::Object(serde_json::Map::new())
                        } else {
                            serde_json::from_str(input_json).map_err(|e| {
                                AnthropicError::Serde(format!("tool input JSON: {e}"))
                            })?
                        };
                        Ok(ContentBlock::ToolUse {
                            id: id.clone(),
                            name: name.clone(),
                            input,
                        })
                    }
                })
                .collect::<Result<Vec<_>, AnthropicError>>()?;

            let usage = if self.input_tokens.is_some() || self.output_tokens.is_some() {
                Some(crate::types::common::Usage {
                    input_tokens: self.input_tokens,
                    output_tokens: self.output_tokens,
                    cache_creation_input_tokens: self.cache_creation_input_tokens,
                    cache_read_input_tokens: self.cache_read_input_tokens,
                })
            } else {
                None
            };

            Ok(MessagesCreateResponse {
                id: self.id.clone().unwrap_or_default(),
                kind: "message".to_string(),
                role: self.role.clone().unwrap_or(MessageRole::Assistant),
                content,
                model: self.model.clone().unwrap_or_default(),
                stop_reason: self.stop_reason.clone(),
                usage,
            })
        }

        /// Get current accumulated text (convenience method)
        ///
        /// Returns concatenated text from all text blocks.
        #[must_use]
        pub fn current_text(&self) -> String {
            self.content_blocks
                .iter()
                .filter_map(|block| match block {
                    AccumulatorBlock::Text(text) => Some(text.as_str()),
                    AccumulatorBlock::ToolUse { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("")
        }
    }

    // =========================================================================
    // Stream Creation
    // =========================================================================

    /// Create an event stream from a reqwest Response
    ///
    /// This function converts the response body into a stream of parsed events.
    /// The stream owns the response and will close the connection when dropped.
    #[must_use]
    pub fn event_stream_from_response(response: reqwest::Response) -> EventStream {
        use futures::StreamExt;

        let byte_stream = response.bytes_stream();

        Box::pin(futures::stream::unfold(
            (byte_stream, SSEDecoder::new(), Vec::<SseFrame>::new()),
            |(mut stream, mut decoder, mut pending_frames)| async move {
                // First, drain any pending frames
                if let Some(frame) = pending_frames.pop() {
                    match Event::from_frame(&frame) {
                        Ok(event) => {
                            return Some((Ok(event), (stream, decoder, pending_frames)));
                        }
                        Err(e) => {
                            // Skip unknown event types, don't error
                            if e.to_string().contains("Unknown event type") {
                                return Some((
                                    Err(e), // or could skip by recursing
                                    (stream, decoder, pending_frames),
                                ));
                            }
                            return Some((Err(e), (stream, decoder, pending_frames)));
                        }
                    }
                }

                // Get next chunk
                loop {
                    match stream.next().await {
                        Some(Ok(chunk)) => {
                            let mut frames = decoder.push(&chunk);
                            frames.reverse(); // So we can pop from the end
                            pending_frames = frames;

                            if let Some(frame) = pending_frames.pop() {
                                match Event::from_frame(&frame) {
                                    Ok(event) => {
                                        return Some((
                                            Ok(event),
                                            (stream, decoder, pending_frames),
                                        ));
                                    }
                                    Err(e) => {
                                        // For unknown event types, continue to next frame
                                        if e.to_string().contains("Unknown event type") {
                                            continue;
                                        }
                                        return Some((Err(e), (stream, decoder, pending_frames)));
                                    }
                                }
                            }
                            // No complete frames yet, get more data
                        }
                        Some(Err(e)) => {
                            return Some((
                                Err(AnthropicError::Reqwest(e)),
                                (stream, decoder, pending_frames),
                            ));
                        }
                        None => {
                            // Stream ended, flush decoder and ignore errors on final frame
                            if let Some(frame) = decoder.flush()
                                && let Ok(event) = Event::from_frame(&frame)
                            {
                                return Some((Ok(event), (stream, decoder, pending_frames)));
                            }
                            return None;
                        }
                    }
                }
            },
        ))
    }
}

// Placeholder when streaming feature is not enabled
#[cfg(not(feature = "streaming"))]
pub(crate) mod streaming {
    // Empty placeholder
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
    use super::streaming::*;
    use crate::types::content::ContentBlock;

    #[test]
    fn test_sse_decoder_single_event() {
        let mut decoder = SSEDecoder::new();
        let chunk = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n";
        let frames = decoder.push(chunk);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event, Some("message_start".to_string()));
        assert!(frames[0].data.contains("message_start"));
    }

    #[test]
    fn test_sse_decoder_multiline_data() {
        let mut decoder = SSEDecoder::new();
        let chunk = b"event: test\ndata: line1\ndata: line2\n\n";
        let frames = decoder.push(chunk);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, "line1\nline2");
    }

    #[test]
    fn test_sse_decoder_split_chunks() {
        let mut decoder = SSEDecoder::new();
        let frames1 = decoder.push(b"event: test\nda");
        assert!(frames1.is_empty());
        let frames2 = decoder.push(b"ta: hello\n\n");
        assert_eq!(frames2.len(), 1);
        assert_eq!(frames2[0].event, Some("test".to_string()));
        assert_eq!(frames2[0].data, "hello");
    }

    #[test]
    fn test_event_mapping_message_start() {
        let frame = SseFrame {
            event: Some("message_start".to_string()),
            data: r#"{"type":"message_start","message":{"id":"msg_123","type":"message","role":"assistant","model":"claude-3-5-sonnet","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}"#.to_string(),
        };
        let event = Event::from_frame(&frame).unwrap();
        match event {
            Event::MessageStart { message } => {
                assert_eq!(message.id, "msg_123");
                assert_eq!(message.model, "claude-3-5-sonnet");
            }
            _ => panic!("Expected MessageStart"),
        }
    }

    #[test]
    fn test_event_mapping_content_block_delta() {
        let frame = SseFrame {
            event: Some("content_block_delta".to_string()),
            data: r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#.to_string(),
        };
        let event = Event::from_frame(&frame).unwrap();
        match event {
            Event::ContentBlockDelta { index, delta } => {
                assert_eq!(index, 0);
                match delta {
                    ContentBlockDeltaData::TextDelta { text } => {
                        assert_eq!(text, "Hello");
                    }
                    _ => panic!("Expected TextDelta"),
                }
            }
            _ => panic!("Expected ContentBlockDelta"),
        }
    }

    #[test]
    fn test_accumulator_text_blocks() {
        let mut acc = Accumulator::new();

        // message_start
        let event1 = Event::MessageStart {
            message: MessageStartPayload {
                id: "msg_test".to_string(),
                kind: "message".to_string(),
                role: crate::types::content::MessageRole::Assistant,
                model: "claude".to_string(),
                content: vec![],
                stop_reason: None,
                stop_sequence: None,
                usage: Some(MessageStartUsage {
                    input_tokens: 10,
                    output_tokens: 0,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                }),
            },
        };
        assert!(acc.apply(&event1).unwrap().is_none());

        // content_block_start
        let event2 = Event::ContentBlockStart {
            index: 0,
            content_block: ContentBlockStartData::Text {
                text: String::new(),
            },
        };
        assert!(acc.apply(&event2).unwrap().is_none());

        // content_block_delta
        let event3 = Event::ContentBlockDelta {
            index: 0,
            delta: ContentBlockDeltaData::TextDelta {
                text: "Hello, ".to_string(),
            },
        };
        assert!(acc.apply(&event3).unwrap().is_none());

        let event4 = Event::ContentBlockDelta {
            index: 0,
            delta: ContentBlockDeltaData::TextDelta {
                text: "world!".to_string(),
            },
        };
        assert!(acc.apply(&event4).unwrap().is_none());
        assert_eq!(acc.current_text(), "Hello, world!");

        // content_block_stop
        let event5 = Event::ContentBlockStop { index: 0 };
        assert!(acc.apply(&event5).unwrap().is_none());

        // message_delta
        let event6 = Event::MessageDelta {
            delta: MessageDeltaPayload {
                stop_reason: Some("end_turn".to_string()),
                stop_sequence: None,
            },
            usage: Some(MessageDeltaUsage { output_tokens: 3 }),
        };
        assert!(acc.apply(&event6).unwrap().is_none());

        // message_stop
        let event7 = Event::MessageStop;
        let response = acc.apply(&event7).unwrap().unwrap();

        assert_eq!(response.id, "msg_test");
        assert_eq!(response.content.len(), 1);
        match &response.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Hello, world!"),
            ContentBlock::ToolUse { .. } => panic!("Expected Text block"),
        }
        assert_eq!(response.stop_reason, Some("end_turn".to_string()));
    }

    #[test]
    fn test_accumulator_tool_use() {
        let mut acc = Accumulator::new();

        // message_start
        acc.apply(&Event::MessageStart {
            message: MessageStartPayload {
                id: "msg_tool".to_string(),
                kind: "message".to_string(),
                role: crate::types::content::MessageRole::Assistant,
                model: "claude".to_string(),
                content: vec![],
                stop_reason: None,
                stop_sequence: None,
                usage: None,
            },
        })
        .unwrap();

        // content_block_start (tool_use)
        acc.apply(&Event::ContentBlockStart {
            index: 0,
            content_block: ContentBlockStartData::ToolUse {
                id: "tool_123".to_string(),
                name: "get_weather".to_string(),
                input: serde_json::json!({}),
            },
        })
        .unwrap();

        // input_json_delta
        acc.apply(&Event::ContentBlockDelta {
            index: 0,
            delta: ContentBlockDeltaData::InputJsonDelta {
                partial_json: r#"{"city":"#.to_string(),
            },
        })
        .unwrap();

        acc.apply(&Event::ContentBlockDelta {
            index: 0,
            delta: ContentBlockDeltaData::InputJsonDelta {
                partial_json: r#""Paris"}"#.to_string(),
            },
        })
        .unwrap();

        // content_block_stop
        acc.apply(&Event::ContentBlockStop { index: 0 }).unwrap();

        // message_delta
        acc.apply(&Event::MessageDelta {
            delta: MessageDeltaPayload {
                stop_reason: Some("tool_use".to_string()),
                stop_sequence: None,
            },
            usage: None,
        })
        .unwrap();

        // message_stop
        let response = acc.apply(&Event::MessageStop).unwrap().unwrap();

        assert_eq!(response.content.len(), 1);
        match &response.content[0] {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "tool_123");
                assert_eq!(name, "get_weather");
                assert_eq!(input["city"], "Paris");
            }
            ContentBlock::Text { .. } => panic!("Expected ToolUse block"),
        }
    }
}
