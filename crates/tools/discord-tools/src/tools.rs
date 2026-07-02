use crate::DiscordTools;
use crate::DiscordToolsError;
use crate::models::DiscordSearchMessagesInput;
use crate::models::DiscordSearchMessagesOutput;
use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use agentic_tools_core::ToolRegistry;
use futures::future::BoxFuture;
use std::sync::Arc;

#[derive(Clone)]
pub struct DiscordSearchMessagesTool {
    discord: Arc<DiscordTools>,
}

impl DiscordSearchMessagesTool {
    pub fn new(discord: Arc<DiscordTools>) -> Self {
        Self { discord }
    }
}

impl Tool for DiscordSearchMessagesTool {
    type Input = DiscordSearchMessagesInput;
    type Output = DiscordSearchMessagesOutput;

    const NAME: &'static str = "discord_search_messages";
    const DESCRIPTION: &'static str = "Search messages in the configured Discord guild using Discord's indexed guild-wide search. Input: query (required) plus optional limit/offset/channel_id/author_id. Limit is clamped to 1..=25; offset clamped to <=9975. If Discord returns HTTP 202 (indexing), the tool retries once, then returns a retryable error.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let discord = Arc::clone(&self.discord);
        Box::pin(async move {
            discord
                .search_messages(input)
                .await
                .map_err(|error| map_discord_error(&error))
        })
    }
}

pub fn build_registry(discord: Arc<DiscordTools>) -> ToolRegistry {
    ToolRegistry::builder()
        .register::<DiscordSearchMessagesTool, ()>(DiscordSearchMessagesTool::new(discord))
        .finish()
}

fn map_discord_error(error: &DiscordToolsError) -> ToolError {
    match error {
        DiscordToolsError::MissingBotToken
        | DiscordToolsError::MissingGuildId
        | DiscordToolsError::InvalidGuildId(_)
        | DiscordToolsError::EmptyQuery
        | DiscordToolsError::InvalidChannelId(_)
        | DiscordToolsError::InvalidAuthorId(_) => ToolError::InvalidInput(error.to_string()),
        DiscordToolsError::Permission(_) => ToolError::Permission(error.to_string()),
        DiscordToolsError::IndexingInProgress | DiscordToolsError::External(_) => {
            ToolError::External(error.to_string())
        }
        DiscordToolsError::InvalidBaseUrl(_) | DiscordToolsError::Internal(_) => {
            ToolError::Internal(error.to_string())
        }
    }
}
