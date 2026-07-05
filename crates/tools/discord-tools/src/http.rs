use crate::DiscordToolsError;
use agentic_config::types::DiscordServiceConfig;
use percent_encoding::NON_ALPHANUMERIC;
use percent_encoding::utf8_percent_encode;
use std::time::Duration;
use tokio::time::sleep;
use twilight_http::Client as TwilightClient;
use twilight_http::error::ErrorType;
use twilight_http::request::Method;
use twilight_http::request::RequestBuilder;

const INDEXING_RETRY_DELAY: Duration = Duration::from_millis(500);

pub struct DiscordClient {
    client: TwilightClient,
}

#[derive(Debug, Clone)]
pub struct GuildMessageSearchParams {
    pub content: String,
    pub limit: u16,
    pub offset: u16,
    pub channel_id: Option<u64>,
    pub author_id: Option<u64>,
}

impl DiscordClient {
    pub fn new(
        bot_token: String,
        config: &DiscordServiceConfig,
    ) -> Result<Self, DiscordToolsError> {
        let mut builder = TwilightClient::builder().token(bot_token);

        if config.request_timeout_secs != 0 {
            builder = builder.timeout(Duration::from_secs(config.request_timeout_secs));
        }

        let base = config.base_url.trim();
        let (use_http, rest) = if let Some(rest) = base.strip_prefix("http://") {
            (true, rest)
        } else if let Some(rest) = base.strip_prefix("https://") {
            (false, rest)
        } else {
            return Err(DiscordToolsError::InvalidBaseUrl(base.to_string()));
        };

        let host_port = rest.split('/').next().unwrap_or(rest).trim();
        if host_port.is_empty() {
            return Err(DiscordToolsError::InvalidBaseUrl(base.to_string()));
        }

        if use_http || host_port != "discord.com" {
            builder = builder.proxy(host_port.to_string(), use_http);
        }

        Ok(Self {
            client: builder.build(),
        })
    }

    pub async fn search_guild_messages_with_index_retry(
        &self,
        guild_id: u64,
        params: &GuildMessageSearchParams,
    ) -> Result<serde_json::Value, DiscordToolsError> {
        let (status, body) = self.search_once(guild_id, params).await?;
        if status == 202 {
            sleep(INDEXING_RETRY_DELAY).await;
            let (retry_status, retry_body) = self.search_once(guild_id, params).await?;
            if retry_status == 202 {
                return Err(DiscordToolsError::IndexingInProgress);
            }

            return Ok(retry_body);
        }

        Ok(body)
    }

    async fn search_once(
        &self,
        guild_id: u64,
        params: &GuildMessageSearchParams,
    ) -> Result<(u16, serde_json::Value), DiscordToolsError> {
        let mut query = vec![
            format!(
                "content={}",
                utf8_percent_encode(&params.content, NON_ALPHANUMERIC)
            ),
            format!("limit={}", params.limit),
            format!("offset={}", params.offset),
        ];

        if let Some(channel_id) = params.channel_id {
            query.push(format!("channel_id={channel_id}"));
        }
        if let Some(author_id) = params.author_id {
            query.push(format!("author_id={author_id}"));
        }

        let path = format!("guilds/{guild_id}/messages/search?{}", query.join("&"));
        let request = RequestBuilder::raw(Method::Get, path)
            .build()
            .map_err(|error| DiscordToolsError::Internal(error.to_string()))?;
        let response = self
            .client
            .request::<serde_json::Value>(request)
            .await
            .map_err(|error| map_twilight_error(&error))?;
        let status = response.status().get();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| DiscordToolsError::External(error.to_string()))?;
        let body = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .map_err(|error| DiscordToolsError::External(error.to_string()))?
        };

        Ok((status, body))
    }
}

fn map_twilight_error(error: &twilight_http::error::Error) -> DiscordToolsError {
    match error.kind() {
        ErrorType::Response { status, .. } => match status.get() {
            401 | 403 => DiscordToolsError::Permission(error.to_string()),
            _ => DiscordToolsError::External(error.to_string()),
        },
        ErrorType::Unauthorized => DiscordToolsError::Permission(
            "discord bot token was rejected by the Discord API".into(),
        ),
        ErrorType::RequestTimedOut => DiscordToolsError::External(format!("timeout: {error}")),
        _ => DiscordToolsError::External(error.to_string()),
    }
}
