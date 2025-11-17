use crate::{
    client::Client,
    config::Config,
    error::AnthropicError,
    types::common::validate_mixed_ttl_order,
    types::messages::{
        ContentBlock, MessageTokensCountRequest, MessageTokensCountResponse, MessagesCreateRequest,
        MessagesCreateResponse,
    },
};

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
        // Validate TTL ordering across system+messages content blocks
        let mut ttls = Vec::new();

        if let Some(system) = &req.system {
            for block in system {
                if let ContentBlock::Text {
                    cache_control: Some(cc),
                    ..
                } = block
                    && let Some(ttl) = &cc.ttl
                {
                    ttls.push(ttl.clone());
                }
            }
        }

        for message in &req.messages {
            for block in &message.content {
                if let ContentBlock::Text {
                    cache_control: Some(cc),
                    ..
                } = block
                    && let Some(ttl) = &cc.ttl
                {
                    ttls.push(ttl.clone());
                }
            }
        }

        if !validate_mixed_ttl_order(ttls) {
            return Err(AnthropicError::Config(
                "Invalid cache_control TTL ordering: 1h must precede 5m".into(),
            ));
        }

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
}

// Add to client
impl<C: Config> crate::Client<C> {
    /// Returns the Messages API resource
    #[must_use]
    pub const fn messages(&self) -> Messages<'_, C> {
        Messages::new(self)
    }
}
