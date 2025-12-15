use crate::{
    client::Client,
    config::Config,
    error::AnthropicError,
    types::common::validate_mixed_ttl_order,
    types::content::{ContentBlockParam, MessageContentParam, SystemParam},
    types::messages::{
        MessageTokensCountRequest, MessageTokensCountResponse, MessagesCreateRequest,
        MessagesCreateResponse,
    },
};

/// Validate a messages create request
///
/// Checks TTL ordering across system+messages content blocks and validates sampling parameters.
fn validate_messages_create_request(req: &MessagesCreateRequest) -> Result<(), AnthropicError> {
    // Validate TTL ordering across system+messages content blocks
    let mut ttls = Vec::new();

    // Scan system blocks
    if let Some(system) = &req.system
        && let SystemParam::Blocks(blocks) = system
    {
        for tb in blocks {
            if let Some(cc) = &tb.cache_control
                && let Some(ttl) = &cc.ttl
            {
                ttls.push(ttl.clone());
            }
        }
    }

    // Scan message blocks
    for message in &req.messages {
        if let MessageContentParam::Blocks(blocks) = &message.content {
            for block in blocks {
                match block {
                    ContentBlockParam::Text {
                        cache_control: Some(cc),
                        ..
                    }
                    | ContentBlockParam::Image {
                        cache_control: Some(cc),
                        ..
                    }
                    | ContentBlockParam::Document {
                        cache_control: Some(cc),
                        ..
                    }
                    | ContentBlockParam::ToolResult {
                        cache_control: Some(cc),
                        ..
                    } => {
                        if let Some(ttl) = &cc.ttl {
                            ttls.push(ttl.clone());
                        }
                    }
                    _ => {}
                }
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
