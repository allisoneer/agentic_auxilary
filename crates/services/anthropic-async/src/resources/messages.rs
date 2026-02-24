use crate::{
    client::Client,
    config::Config,
    error::AnthropicError,
    types::common::{CacheControl, CacheTtl, validate_mixed_ttl_order},
    types::content::{
        ContentBlockParam, MessageContentParam, SystemParam, ToolResultContent,
        ToolResultContentBlock,
    },
    types::messages::{
        MessageTokensCountRequest, MessageTokensCountResponse, MessagesCreateRequest,
        MessagesCreateResponse,
    },
};

// ============================================================================
// TTL Validation Helpers
// ============================================================================

/// Push TTL to the collection if `cache_control` contains a TTL
fn push_ttl(ttls: &mut Vec<CacheTtl>, cache_control: Option<&CacheControl>) {
    if let Some(ttl) = cache_control.and_then(|cc| cc.ttl.clone()) {
        ttls.push(ttl);
    }
}

/// Collect TTLs from nested `ToolResultContent` blocks
fn collect_tool_result_content_ttls(ttls: &mut Vec<CacheTtl>, content: Option<&ToolResultContent>) {
    let Some(content) = content else { return };
    if let ToolResultContent::Blocks(blocks) = content {
        for block in blocks {
            match block {
                ToolResultContentBlock::Text { cache_control, .. }
                | ToolResultContentBlock::Image { cache_control, .. } => {
                    push_ttl(ttls, cache_control.as_ref());
                }
            }
        }
    }
}

/// Collect TTLs from a `ContentBlockParam`, including nested content
fn collect_block_param_ttls(ttls: &mut Vec<CacheTtl>, block: &ContentBlockParam) {
    match block {
        ContentBlockParam::Text { cache_control, .. }
        | ContentBlockParam::Image { cache_control, .. }
        | ContentBlockParam::Document { cache_control, .. }
        | ContentBlockParam::ToolUse { cache_control, .. }
        | ContentBlockParam::ServerToolUse { cache_control, .. }
        | ContentBlockParam::SearchResult { cache_control, .. }
        | ContentBlockParam::WebSearchToolResult { cache_control, .. } => {
            push_ttl(ttls, cache_control.as_ref());
        }
        ContentBlockParam::ToolResult {
            cache_control,
            content,
            ..
        } => {
            push_ttl(ttls, cache_control.as_ref());
            collect_tool_result_content_ttls(ttls, content.as_ref());
        }
        ContentBlockParam::Thinking { .. } | ContentBlockParam::RedactedThinking { .. } => {
            // No cache_control on thinking blocks
        }
    }
}

/// Validate a messages create request
///
/// Checks TTL ordering across all cacheable locations (system, tools, messages)
/// and validates sampling parameters.
///
/// # TTL Validation Locations (12 total)
///
/// 1. `SystemParam::Blocks` → `TextBlockParam.cache_control.ttl`
/// 2. `Tool.cache_control.ttl` (tool definitions)
/// 3. `ContentBlockParam::{Text, Image, Document, ToolUse, ServerToolUse, SearchResult,
///    WebSearchToolResult}.cache_control.ttl`
/// 4. `ContentBlockParam::ToolResult.cache_control.ttl`
/// 5. `ToolResultContentBlock::{Text, Image}.cache_control.ttl` (nested inside `ToolResult`)
fn validate_messages_create_request(req: &MessagesCreateRequest) -> Result<(), AnthropicError> {
    // Validate TTL ordering across all cacheable locations
    // Order: system → tools → messages (canonical traversal)
    let mut ttls = Vec::new();

    // 1. Scan system blocks
    if let Some(system) = &req.system
        && let SystemParam::Blocks(blocks) = system
    {
        for tb in blocks {
            push_ttl(&mut ttls, tb.cache_control.as_ref());
        }
    }

    // 2. Scan tool definitions
    if let Some(tools) = &req.tools {
        for tool in tools {
            push_ttl(&mut ttls, tool.cache_control.as_ref());
        }
    }

    // 3-12. Scan message content blocks (including nested ToolResult blocks)
    for message in &req.messages {
        if let MessageContentParam::Blocks(blocks) = &message.content {
            for block in blocks {
                collect_block_param_ttls(&mut ttls, block);
            }
        }
    }

    if !validate_mixed_ttl_order(ttls) {
        return Err(AnthropicError::Config(
            "Invalid cache_control TTL ordering: 1h must precede 5m".into(),
        ));
    }

    // Validate sampling parameters
    if let Some(t) = req.temperature
        && !(0.0..=1.0).contains(&t)
    {
        return Err(AnthropicError::Config(format!(
            "Invalid temperature {t}: must be in [0.0, 1.0]"
        )));
    }

    if let Some(p) = req.top_p
        && (!(0.0..=1.0).contains(&p) || p == 0.0)
    {
        return Err(AnthropicError::Config(format!(
            "Invalid top_p {p}: must be in (0.0, 1.0]"
        )));
    }

    if let Some(k) = req.top_k
        && k < 1
    {
        return Err(AnthropicError::Config(format!(
            "Invalid top_k {k}: must be >= 1"
        )));
    }

    if req.max_tokens == 0 {
        return Err(AnthropicError::Config(
            "max_tokens must be greater than 0".into(),
        ));
    }

    Ok(())
}

/// API resource for the `/v1/messages` endpoints
///
/// Provides methods to create messages and count tokens.
pub struct Messages<'c, C: Config> {
    client: &'c Client<C>,
}

impl<'c, C: Config> Messages<'c, C> {
    /// Creates a new Messages resource
    #[must_use]
    pub const fn new(client: &'c Client<C>) -> Self {
        Self { client }
    }

    /// Create a new message
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The request fails to send
    /// - The `cache_control` TTL ordering is invalid (1h must precede 5m)
    /// - The API returns an error
    pub async fn create(
        &self,
        req: MessagesCreateRequest,
    ) -> Result<MessagesCreateResponse, AnthropicError> {
        // Centralized validation
        validate_messages_create_request(&req)?;

        self.client.post("/v1/messages", req).await
    }

    /// Count tokens for a message request
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The request fails to send
    /// - The API returns an error
    pub async fn count_tokens(
        &self,
        req: MessageTokensCountRequest,
    ) -> Result<MessageTokensCountResponse, AnthropicError> {
        // No TTL validation needed for token counting
        self.client.post("/v1/messages/count_tokens", req).await
    }

    /// Create a new message with streaming response
    ///
    /// Returns a stream of SSE events that can be processed as they arrive.
    /// The request will automatically have `stream: true` set.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use futures::StreamExt;
    ///
    /// let mut stream = client.messages().create_stream(req).await?;
    /// while let Some(event) = stream.next().await {
    ///     match event? {
    ///         Event::ContentBlockDelta { delta, .. } => {
    ///             if let ContentBlockDeltaData::TextDelta { text } = delta {
    ///                 print!("{}", text);
    ///             }
    ///         }
    ///         Event::MessageStop => break,
    ///         _ => {}
    ///     }
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The request fails to send
    /// - The API returns an error (non-2xx status)
    #[cfg(feature = "streaming")]
    pub async fn create_stream(
        &self,
        mut req: MessagesCreateRequest,
    ) -> Result<crate::streaming::EventStream, AnthropicError> {
        // Force streaming mode
        req.stream = Some(true);

        // Centralized validation
        validate_messages_create_request(&req)?;

        let response = self.client.post_stream("/v1/messages", req).await?;
        Ok(crate::sse::streaming::event_stream_from_response(response))
    }
}

// Add to client
impl<C: Config> crate::Client<C> {
    /// Returns the Messages API resource
    #[must_use]
    pub const fn messages(&self) -> Messages<'_, C> {
        Messages::new(self)
    }
}

// ============================================================================
// TTL Validation Tests (12 cacheable locations)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        content::{
            DocumentSource, ImageSource, MessageParam, MessageRole, TextBlockParam,
            ToolResultContent, ToolResultContentBlock,
        },
        tools::Tool,
    };

    /// Create a base request with minimal valid fields
    fn base_req(messages: Vec<MessageParam>) -> MessagesCreateRequest {
        MessagesCreateRequest {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 16,
            messages,
            ..Default::default()
        }
    }

    /// Create a user message with content blocks
    fn user_blocks(blocks: Vec<ContentBlockParam>) -> MessageParam {
        MessageParam {
            role: MessageRole::User,
            content: MessageContentParam::Blocks(blocks),
        }
    }

    /// Assert that a request with mixed TTLs in wrong order (5m before 1h) errors
    fn assert_ttl_order_err(req: &MessagesCreateRequest) {
        let err = validate_messages_create_request(req).unwrap_err();
        match err {
            AnthropicError::Config(msg) => {
                assert!(
                    msg.contains("TTL ordering"),
                    "Expected TTL ordering error, got: {msg}"
                );
            }
            _ => panic!("Expected AnthropicError::Config, got {err:?}"),
        }
    }

    /// Assert that a request passes validation
    fn assert_valid(req: &MessagesCreateRequest) {
        assert!(
            validate_messages_create_request(req).is_ok(),
            "Expected valid request"
        );
    }

    // -------------------------------------------------------------------------
    // Location 1: SystemParam::Blocks TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_system_block_5m_then_1h_fails() {
        let mut req = base_req(vec![user_blocks(vec![ContentBlockParam::Text {
            text: "hi".into(),
            citations: None,
            cache_control: None,
        }])]);
        req.system = Some(SystemParam::Blocks(vec![
            TextBlockParam::with_cache_control("first", CacheControl::ephemeral_5m()),
            TextBlockParam::with_cache_control("second", CacheControl::ephemeral_1h()),
        ]));
        assert_ttl_order_err(&req);
    }

    #[test]
    fn ttl_system_block_1h_then_5m_passes() {
        let mut req = base_req(vec![user_blocks(vec![ContentBlockParam::Text {
            text: "hi".into(),
            citations: None,
            cache_control: None,
        }])]);
        req.system = Some(SystemParam::Blocks(vec![
            TextBlockParam::with_cache_control("first", CacheControl::ephemeral_1h()),
            TextBlockParam::with_cache_control("second", CacheControl::ephemeral_5m()),
        ]));
        assert_valid(&req);
    }

    // -------------------------------------------------------------------------
    // Location 2: Tool.cache_control TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_tool_definition_5m_then_1h_fails() {
        let mut req = base_req(vec![user_blocks(vec![ContentBlockParam::Text {
            text: "hi".into(),
            citations: None,
            cache_control: None,
        }])]);
        req.tools = Some(vec![
            Tool {
                name: "tool1".into(),
                description: None,
                input_schema: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_5m()),
                strict: None,
            },
            Tool {
                name: "tool2".into(),
                description: None,
                input_schema: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_1h()),
                strict: None,
            },
        ]);
        assert_ttl_order_err(&req);
    }

    #[test]
    fn ttl_tool_definition_1h_then_5m_passes() {
        let mut req = base_req(vec![user_blocks(vec![ContentBlockParam::Text {
            text: "hi".into(),
            citations: None,
            cache_control: None,
        }])]);
        req.tools = Some(vec![
            Tool {
                name: "tool1".into(),
                description: None,
                input_schema: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_1h()),
                strict: None,
            },
            Tool {
                name: "tool2".into(),
                description: None,
                input_schema: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_5m()),
                strict: None,
            },
        ]);
        assert_valid(&req);
    }

    // -------------------------------------------------------------------------
    // Location 3: ContentBlockParam::Text TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_text_block_5m_then_1h_fails() {
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::Text {
                text: "first".into(),
                citations: None,
                cache_control: Some(CacheControl::ephemeral_5m()),
            },
            ContentBlockParam::Text {
                text: "second".into(),
                citations: None,
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
        ])]);
        assert_ttl_order_err(&req);
    }

    #[test]
    fn ttl_text_block_1h_then_5m_passes() {
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::Text {
                text: "first".into(),
                citations: None,
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
            ContentBlockParam::Text {
                text: "second".into(),
                citations: None,
                cache_control: Some(CacheControl::ephemeral_5m()),
            },
        ])]);
        assert_valid(&req);
    }

    // -------------------------------------------------------------------------
    // Location 4: ContentBlockParam::Image TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_image_block_5m_then_1h_fails() {
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::Image {
                source: ImageSource::Base64 {
                    media_type: "image/png".into(),
                    data: String::new(),
                },
                cache_control: Some(CacheControl::ephemeral_5m()),
            },
            ContentBlockParam::Image {
                source: ImageSource::Base64 {
                    media_type: "image/png".into(),
                    data: String::new(),
                },
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
        ])]);
        assert_ttl_order_err(&req);
    }

    // -------------------------------------------------------------------------
    // Location 5: ContentBlockParam::Document TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_document_block_5m_then_1h_fails() {
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::Document {
                source: DocumentSource::Base64 {
                    media_type: "application/pdf".into(),
                    data: String::new(),
                },
                cache_control: Some(CacheControl::ephemeral_5m()),
            },
            ContentBlockParam::Document {
                source: DocumentSource::Base64 {
                    media_type: "application/pdf".into(),
                    data: String::new(),
                },
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
        ])]);
        assert_ttl_order_err(&req);
    }

    // -------------------------------------------------------------------------
    // Location 6: ContentBlockParam::ToolUse TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_tool_use_block_5m_then_1h_fails() {
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::ToolUse {
                id: "id1".into(),
                name: "tool".into(),
                input: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_5m()),
            },
            ContentBlockParam::ToolUse {
                id: "id2".into(),
                name: "tool".into(),
                input: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
        ])]);
        assert_ttl_order_err(&req);
    }

    #[test]
    fn ttl_tool_use_block_1h_then_5m_passes() {
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::ToolUse {
                id: "id1".into(),
                name: "tool".into(),
                input: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
            ContentBlockParam::ToolUse {
                id: "id2".into(),
                name: "tool".into(),
                input: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_5m()),
            },
        ])]);
        assert_valid(&req);
    }

    // -------------------------------------------------------------------------
    // Location 7: ContentBlockParam::ServerToolUse TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_server_tool_use_block_5m_then_1h_fails() {
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::ServerToolUse {
                id: "id1".into(),
                name: "web_search".into(),
                input: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_5m()),
            },
            ContentBlockParam::ServerToolUse {
                id: "id2".into(),
                name: "web_search".into(),
                input: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
        ])]);
        assert_ttl_order_err(&req);
    }

    // -------------------------------------------------------------------------
    // Location 8: ContentBlockParam::SearchResult TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_search_result_block_5m_then_1h_fails() {
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::SearchResult {
                content: vec![],
                source: "https://example.com".into(),
                title: "Result".into(),
                citations: None,
                cache_control: Some(CacheControl::ephemeral_5m()),
            },
            ContentBlockParam::SearchResult {
                content: vec![],
                source: "https://example.com".into(),
                title: "Result".into(),
                citations: None,
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
        ])]);
        assert_ttl_order_err(&req);
    }

    // -------------------------------------------------------------------------
    // Location 9: ContentBlockParam::WebSearchToolResult TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_web_search_tool_result_block_5m_then_1h_fails() {
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::WebSearchToolResult {
                tool_use_id: "id1".into(),
                content: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_5m()),
            },
            ContentBlockParam::WebSearchToolResult {
                tool_use_id: "id2".into(),
                content: serde_json::json!({}),
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
        ])]);
        assert_ttl_order_err(&req);
    }

    // -------------------------------------------------------------------------
    // Location 10: ContentBlockParam::ToolResult TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_tool_result_block_5m_then_1h_fails() {
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::ToolResult {
                tool_use_id: "id1".into(),
                content: None,
                is_error: None,
                cache_control: Some(CacheControl::ephemeral_5m()),
            },
            ContentBlockParam::ToolResult {
                tool_use_id: "id2".into(),
                content: None,
                is_error: None,
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
        ])]);
        assert_ttl_order_err(&req);
    }

    // -------------------------------------------------------------------------
    // Location 11-12: Nested ToolResultContentBlock::{Text, Image} TTL validation
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_nested_tool_result_text_5m_then_1h_fails() {
        let req = base_req(vec![user_blocks(vec![ContentBlockParam::ToolResult {
            tool_use_id: "id1".into(),
            content: Some(ToolResultContent::Blocks(vec![
                ToolResultContentBlock::Text {
                    text: "first".into(),
                    cache_control: Some(CacheControl::ephemeral_5m()),
                },
                ToolResultContentBlock::Text {
                    text: "second".into(),
                    cache_control: Some(CacheControl::ephemeral_1h()),
                },
            ])),
            is_error: None,
            cache_control: None,
        }])]);
        assert_ttl_order_err(&req);
    }

    #[test]
    fn ttl_nested_tool_result_image_5m_then_1h_fails() {
        let req = base_req(vec![user_blocks(vec![ContentBlockParam::ToolResult {
            tool_use_id: "id1".into(),
            content: Some(ToolResultContent::Blocks(vec![
                ToolResultContentBlock::Image {
                    source: ImageSource::Base64 {
                        media_type: "image/png".into(),
                        data: String::new(),
                    },
                    cache_control: Some(CacheControl::ephemeral_5m()),
                },
                ToolResultContentBlock::Image {
                    source: ImageSource::Base64 {
                        media_type: "image/png".into(),
                        data: String::new(),
                    },
                    cache_control: Some(CacheControl::ephemeral_1h()),
                },
            ])),
            is_error: None,
            cache_control: None,
        }])]);
        assert_ttl_order_err(&req);
    }

    #[test]
    fn ttl_nested_tool_result_1h_then_5m_passes() {
        let req = base_req(vec![user_blocks(vec![ContentBlockParam::ToolResult {
            tool_use_id: "id1".into(),
            content: Some(ToolResultContent::Blocks(vec![
                ToolResultContentBlock::Text {
                    text: "first".into(),
                    cache_control: Some(CacheControl::ephemeral_1h()),
                },
                ToolResultContentBlock::Text {
                    text: "second".into(),
                    cache_control: Some(CacheControl::ephemeral_5m()),
                },
            ])),
            is_error: None,
            cache_control: None,
        }])]);
        assert_valid(&req);
    }

    // -------------------------------------------------------------------------
    // Cross-location TTL ordering (canonical traversal: system → tools → messages)
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_system_5m_tool_1h_fails() {
        // System comes before tools in canonical traversal
        let mut req = base_req(vec![user_blocks(vec![ContentBlockParam::Text {
            text: "hi".into(),
            citations: None,
            cache_control: None,
        }])]);
        req.system = Some(SystemParam::Blocks(vec![
            TextBlockParam::with_cache_control("sys", CacheControl::ephemeral_5m()),
        ]));
        req.tools = Some(vec![Tool {
            name: "tool".into(),
            description: None,
            input_schema: serde_json::json!({}),
            cache_control: Some(CacheControl::ephemeral_1h()),
            strict: None,
        }]);
        assert_ttl_order_err(&req);
    }

    #[test]
    fn ttl_tool_5m_message_1h_fails() {
        // Tools come before messages in canonical traversal
        let mut req = base_req(vec![user_blocks(vec![ContentBlockParam::Text {
            text: "hi".into(),
            citations: None,
            cache_control: Some(CacheControl::ephemeral_1h()),
        }])]);
        req.tools = Some(vec![Tool {
            name: "tool".into(),
            description: None,
            input_schema: serde_json::json!({}),
            cache_control: Some(CacheControl::ephemeral_5m()),
            strict: None,
        }]);
        assert_ttl_order_err(&req);
    }

    #[test]
    fn ttl_system_1h_tool_5m_message_none_passes() {
        // Correct order: 1h → 5m across system and tools
        let mut req = base_req(vec![user_blocks(vec![ContentBlockParam::Text {
            text: "hi".into(),
            citations: None,
            cache_control: None,
        }])]);
        req.system = Some(SystemParam::Blocks(vec![
            TextBlockParam::with_cache_control("sys", CacheControl::ephemeral_1h()),
        ]));
        req.tools = Some(vec![Tool {
            name: "tool".into(),
            description: None,
            input_schema: serde_json::json!({}),
            cache_control: Some(CacheControl::ephemeral_5m()),
            strict: None,
        }]);
        assert_valid(&req);
    }

    // -------------------------------------------------------------------------
    // Edge cases: Thinking blocks (no cache_control), String content, no TTLs
    // -------------------------------------------------------------------------

    #[test]
    fn ttl_thinking_blocks_ignored() {
        // Thinking and RedactedThinking have no cache_control
        let req = base_req(vec![user_blocks(vec![
            ContentBlockParam::Thinking {
                thinking: "thinking...".into(),
                signature: "sig".into(),
            },
            ContentBlockParam::RedactedThinking {
                data: "redacted".into(),
            },
            ContentBlockParam::Text {
                text: "hi".into(),
                citations: None,
                cache_control: Some(CacheControl::ephemeral_1h()),
            },
        ])]);
        assert_valid(&req);
    }

    #[test]
    fn ttl_string_content_ignored() {
        // String content (not Blocks) is skipped
        let req = base_req(vec![MessageParam {
            role: MessageRole::User,
            content: MessageContentParam::String("hi".into()),
        }]);
        assert_valid(&req);
    }

    #[test]
    fn ttl_no_cache_control_passes() {
        // Request with no TTLs should pass
        let req = base_req(vec![user_blocks(vec![ContentBlockParam::Text {
            text: "hi".into(),
            citations: None,
            cache_control: None,
        }])]);
        assert_valid(&req);
    }
}
