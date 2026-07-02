pub mod http;
pub mod models;
pub mod tools;

use agentic_config::types::DiscordServiceConfig;
use http::DiscordClient;
use http::GuildMessageSearchParams;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

pub use tools::build_registry;

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 25;
const MAX_OFFSET: u32 = 9_975;

#[derive(Debug, Error)]
pub enum DiscordToolsError {
    #[error("DISCORD_BOT_TOKEN environment variable is not set")]
    MissingBotToken,
    #[error("DISCORD_GUILD_ID environment variable is not set")]
    MissingGuildId,
    #[error("invalid DISCORD_GUILD_ID {0:?}; expected a numeric Discord snowflake")]
    InvalidGuildId(String),
    #[error("query must not be empty")]
    EmptyQuery,
    #[error("invalid channel_id {0:?}; expected a numeric Discord snowflake")]
    InvalidChannelId(String),
    #[error("invalid author_id {0:?}; expected a numeric Discord snowflake")]
    InvalidAuthorId(String),
    #[error("invalid services.discord.base_url {0:?}; expected an http(s) URL")]
    InvalidBaseUrl(String),
    #[error(
        "discord search index is still building (HTTP 202). retry this tool call in a few seconds"
    )]
    IndexingInProgress,
    #[error("discord permission/auth error: {0}")]
    Permission(String),
    #[error("discord API error: {0}")]
    External(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Clone)]
pub struct DiscordTools {
    config: DiscordServiceConfig,
    client_cache: Arc<Mutex<Option<CachedDiscordClient>>>,
}

struct CachedDiscordClient {
    token: String,
    client: Arc<DiscordClient>,
}

impl Default for DiscordTools {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedSearchInput {
    query: String,
    limit: u32,
    offset: u32,
    channel_id: Option<u64>,
    author_id: Option<u64>,
    warnings: Vec<String>,
}

impl DiscordTools {
    pub fn new() -> Self {
        Self::with_config(DiscordServiceConfig::default())
    }

    pub fn with_config(config: DiscordServiceConfig) -> Self {
        Self {
            config,
            client_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn config(&self) -> &DiscordServiceConfig {
        &self.config
    }

    pub async fn search_messages(
        &self,
        input: models::DiscordSearchMessagesInput,
    ) -> Result<models::DiscordSearchMessagesOutput, DiscordToolsError> {
        let bot_token =
            env_trimmed("DISCORD_BOT_TOKEN").ok_or(DiscordToolsError::MissingBotToken)?;
        let guild_id_raw =
            env_trimmed("DISCORD_GUILD_ID").ok_or(DiscordToolsError::MissingGuildId)?;
        let guild_id = parse_snowflake(&guild_id_raw)
            .map_err(|_| DiscordToolsError::InvalidGuildId(guild_id_raw.clone()))?;
        let normalized = normalize_input(input)?;

        let client = self.client_for_token(bot_token).await?;
        let json = client
            .search_guild_messages_with_index_retry(
                guild_id,
                &GuildMessageSearchParams {
                    content: normalized.query.clone(),
                    limit: normalized.limit as u16,
                    offset: normalized.offset as u16,
                    channel_id: normalized.channel_id,
                    author_id: normalized.author_id,
                },
            )
            .await?;

        models::DiscordSearchMessagesOutput::from_search_response(
            guild_id,
            normalized.query,
            normalized.limit,
            normalized.offset,
            normalized.warnings,
            &json,
        )
    }

    async fn client_for_token(
        &self,
        token: String,
    ) -> Result<Arc<DiscordClient>, DiscordToolsError> {
        let mut guard = self.client_cache.lock().await;

        if let Some(cached) = guard.as_ref()
            && cached.token == token
        {
            return Ok(Arc::clone(&cached.client));
        }

        let client = Arc::new(DiscordClient::new(token.clone(), &self.config)?);
        *guard = Some(CachedDiscordClient {
            token,
            client: Arc::clone(&client),
        });

        Ok(client)
    }
}

fn normalize_input(
    input: models::DiscordSearchMessagesInput,
) -> Result<NormalizedSearchInput, DiscordToolsError> {
    let query = input.query.trim().to_string();
    if query.is_empty() {
        return Err(DiscordToolsError::EmptyQuery);
    }

    let mut warnings = Vec::new();

    let requested_limit = input.limit.unwrap_or(DEFAULT_LIMIT);
    let limit = requested_limit.clamp(1, MAX_LIMIT);
    if limit != requested_limit {
        warnings.push(format!(
            "limit clamped from {requested_limit} to {limit} (allowed range: 1..={MAX_LIMIT})"
        ));
    }

    let requested_offset = input.offset.unwrap_or(0);
    let offset = requested_offset.min(MAX_OFFSET);
    if offset != requested_offset {
        warnings.push(format!(
            "offset clamped from {requested_offset} to {offset} (maximum allowed: {MAX_OFFSET})"
        ));
    }

    let channel_id =
        parse_optional_snowflake(input.channel_id, DiscordToolsError::InvalidChannelId)?;
    let author_id = parse_optional_snowflake(input.author_id, DiscordToolsError::InvalidAuthorId)?;

    Ok(NormalizedSearchInput {
        query,
        limit,
        offset,
        channel_id,
        author_id,
        warnings,
    })
}

fn parse_optional_snowflake<E>(
    raw: Option<String>,
    map_err: impl FnOnce(String) -> E + Copy,
) -> Result<Option<u64>, E> {
    raw.map(|value| {
        let trimmed = value.trim().to_string();
        parse_snowflake(&trimmed).map_err(|_| map_err(trimmed))
    })
    .transpose()
}

fn parse_snowflake(raw: &str) -> Result<u64, std::num::ParseIntError> {
    raw.parse::<u64>()
}

fn env_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_input_clamps_and_warns() {
        let normalized = normalize_input(models::DiscordSearchMessagesInput {
            query: "  hello  ".into(),
            limit: Some(99),
            offset: Some(10_000),
            channel_id: Some("123".into()),
            author_id: Some("456".into()),
        })
        .unwrap();

        assert_eq!(normalized.query, "hello");
        assert_eq!(normalized.limit, 25);
        assert_eq!(normalized.offset, 9_975);
        assert_eq!(normalized.channel_id, Some(123));
        assert_eq!(normalized.author_id, Some(456));
        assert_eq!(normalized.warnings.len(), 2);
    }

    #[test]
    fn normalize_input_rejects_empty_query() {
        let err = normalize_input(models::DiscordSearchMessagesInput {
            query: "   ".into(),
            limit: None,
            offset: None,
            channel_id: None,
            author_id: None,
        })
        .unwrap_err();

        assert!(matches!(err, DiscordToolsError::EmptyQuery));
    }

    #[test]
    fn normalize_input_rejects_bad_author_id() {
        let err = normalize_input(models::DiscordSearchMessagesInput {
            query: "hello".into(),
            limit: None,
            offset: None,
            channel_id: None,
            author_id: Some("abc".into()),
        })
        .unwrap_err();

        assert!(matches!(err, DiscordToolsError::InvalidAuthorId(_)));
    }
}
