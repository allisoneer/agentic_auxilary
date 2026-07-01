use anyhow::Result;
use discord_tools::DiscordTools;
use discord_tools::DiscordToolsError;
use discord_tools::models::DiscordSearchMessagesInput;
use discord_tools::test_support::EnvGuard;
use mockito::Matcher;
use mockito::Server;
use serial_test::serial;
use std::sync::Once;

static RUSTLS_PROVIDER: Once = Once::new();

fn install_rustls_provider() {
    RUSTLS_PROVIDER.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

#[tokio::test]
#[serial(env)]
async fn missing_bot_token_is_error() -> Result<()> {
    let _token = EnvGuard::remove("DISCORD_BOT_TOKEN");
    let _guild = EnvGuard::set("DISCORD_GUILD_ID", "123");

    let tools = DiscordTools::new();
    let err = tools
        .search_messages(DiscordSearchMessagesInput {
            query: "hello".into(),
            limit: None,
            offset: None,
            channel_id: None,
            author_id: None,
        })
        .await
        .unwrap_err();

    assert!(matches!(err, DiscordToolsError::MissingBotToken));
    Ok(())
}

#[tokio::test]
#[serial(env)]
async fn missing_guild_id_is_error() -> Result<()> {
    let _token = EnvGuard::set("DISCORD_BOT_TOKEN", "token");
    let _guild = EnvGuard::remove("DISCORD_GUILD_ID");

    let tools = DiscordTools::new();
    let err = tools
        .search_messages(DiscordSearchMessagesInput {
            query: "hello".into(),
            limit: None,
            offset: None,
            channel_id: None,
            author_id: None,
        })
        .await
        .unwrap_err();

    assert!(matches!(err, DiscordToolsError::MissingGuildId));
    Ok(())
}

#[tokio::test]
#[serial(env)]
async fn auth_failure_maps_to_permission_error() -> Result<()> {
    install_rustls_provider();
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/v10/guilds/123/messages/search")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("content".into(), "hello".into()),
            Matcher::UrlEncoded("limit".into(), "10".into()),
            Matcher::UrlEncoded("offset".into(), "0".into()),
        ]))
        .with_status(401)
        .with_header("content-type", "application/json")
        .with_body(r#"{"message":"401: Unauthorized","code":0}"#)
        .create_async()
        .await;

    let _token = EnvGuard::set("DISCORD_BOT_TOKEN", "token");
    let _guild = EnvGuard::set("DISCORD_GUILD_ID", "123");
    let tools = DiscordTools::with_config(agentic_config::types::DiscordServiceConfig {
        base_url: server.url(),
        request_timeout_secs: 5,
    });

    let err = tools
        .search_messages(DiscordSearchMessagesInput {
            query: "hello".into(),
            limit: None,
            offset: None,
            channel_id: None,
            author_id: None,
        })
        .await
        .unwrap_err();

    assert!(matches!(err, DiscordToolsError::Permission(_)));
    Ok(())
}

#[tokio::test]
#[serial(env)]
async fn search_success_parses_hits_and_jump_urls() -> Result<()> {
    install_rustls_provider();
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/v10/guilds/123/messages/search")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("content".into(), "hello".into()),
            Matcher::UrlEncoded("limit".into(), "1".into()),
            Matcher::UrlEncoded("offset".into(), "0".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"total_results":1,"messages":[[{"id":"555","channel_id":"777","content":"hello world","timestamp":"2026-01-01T00:00:00.000Z","author":{"id":"999","username":"alice"}}]]}"#,
        )
        .create_async()
        .await;

    let _token = EnvGuard::set("DISCORD_BOT_TOKEN", "token");
    let _guild = EnvGuard::set("DISCORD_GUILD_ID", "123");
    let tools = DiscordTools::with_config(agentic_config::types::DiscordServiceConfig {
        base_url: server.url(),
        request_timeout_secs: 5,
    });

    let output = tools
        .search_messages(DiscordSearchMessagesInput {
            query: "hello".into(),
            limit: Some(1),
            offset: Some(0),
            channel_id: None,
            author_id: None,
        })
        .await?;

    assert_eq!(output.results.len(), 1);
    assert_eq!(
        output.results[0].jump_url,
        "https://discord.com/channels/123/777/555"
    );
    assert_eq!(output.results[0].author_username.as_deref(), Some("alice"));
    Ok(())
}

#[tokio::test]
#[serial(env)]
async fn limit_and_offset_are_clamped_with_warnings() -> Result<()> {
    install_rustls_provider();
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/v10/guilds/123/messages/search")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("content".into(), "hello".into()),
            Matcher::UrlEncoded("limit".into(), "25".into()),
            Matcher::UrlEncoded("offset".into(), "9975".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"total_results":0,"messages":[]}"#)
        .create_async()
        .await;

    let _token = EnvGuard::set("DISCORD_BOT_TOKEN", "token");
    let _guild = EnvGuard::set("DISCORD_GUILD_ID", "123");
    let tools = DiscordTools::with_config(agentic_config::types::DiscordServiceConfig {
        base_url: server.url(),
        request_timeout_secs: 5,
    });

    let output = tools
        .search_messages(DiscordSearchMessagesInput {
            query: "hello".into(),
            limit: Some(99),
            offset: Some(10_000),
            channel_id: None,
            author_id: None,
        })
        .await?;

    assert_eq!(output.limit, 25);
    assert_eq!(output.offset, 9_975);
    assert_eq!(output.warnings.len(), 2);
    Ok(())
}

#[tokio::test]
#[serial(env)]
async fn channel_and_author_filters_are_emitted() -> Result<()> {
    install_rustls_provider();
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("GET", "/api/v10/guilds/123/messages/search")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("content".into(), "hello".into()),
            Matcher::UrlEncoded("limit".into(), "10".into()),
            Matcher::UrlEncoded("offset".into(), "0".into()),
            Matcher::UrlEncoded("channel_id".into(), "777".into()),
            Matcher::UrlEncoded("author_id".into(), "999".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"total_results":0,"messages":[]}"#)
        .create_async()
        .await;

    let _token = EnvGuard::set("DISCORD_BOT_TOKEN", "token");
    let _guild = EnvGuard::set("DISCORD_GUILD_ID", "123");
    let tools = DiscordTools::with_config(agentic_config::types::DiscordServiceConfig {
        base_url: server.url(),
        request_timeout_secs: 5,
    });

    let output = tools
        .search_messages(DiscordSearchMessagesInput {
            query: "hello".into(),
            limit: None,
            offset: None,
            channel_id: Some("777".into()),
            author_id: Some("999".into()),
        })
        .await?;

    assert!(output.results.is_empty());
    Ok(())
}

#[tokio::test]
#[serial(env)]
async fn indexing_202_retries_once_then_succeeds() -> Result<()> {
    install_rustls_provider();
    let mut server = Server::new_async().await;
    let _first = server
        .mock("GET", "/api/v10/guilds/123/messages/search")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("content".into(), "hello".into()),
            Matcher::UrlEncoded("limit".into(), "10".into()),
            Matcher::UrlEncoded("offset".into(), "0".into()),
        ]))
        .with_status(202)
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":110000,"message":"Index not yet available"}"#)
        .expect(1)
        .create_async()
        .await;
    let _second = server
        .mock("GET", "/api/v10/guilds/123/messages/search")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("content".into(), "hello".into()),
            Matcher::UrlEncoded("limit".into(), "10".into()),
            Matcher::UrlEncoded("offset".into(), "0".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"total_results":0,"messages":[]}"#)
        .expect(1)
        .create_async()
        .await;

    let _token = EnvGuard::set("DISCORD_BOT_TOKEN", "token");
    let _guild = EnvGuard::set("DISCORD_GUILD_ID", "123");
    let tools = DiscordTools::with_config(agentic_config::types::DiscordServiceConfig {
        base_url: server.url(),
        request_timeout_secs: 5,
    });

    let output = tools
        .search_messages(DiscordSearchMessagesInput {
            query: "hello".into(),
            limit: None,
            offset: None,
            channel_id: None,
            author_id: None,
        })
        .await?;

    assert!(output.results.is_empty());
    Ok(())
}

#[tokio::test]
#[serial(env)]
async fn indexing_202_twice_returns_retryable_error() -> Result<()> {
    install_rustls_provider();
    let mut server = Server::new_async().await;
    let _first = server
        .mock("GET", "/api/v10/guilds/123/messages/search")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("content".into(), "hello".into()),
            Matcher::UrlEncoded("limit".into(), "10".into()),
            Matcher::UrlEncoded("offset".into(), "0".into()),
        ]))
        .with_status(202)
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":110000,"message":"Index not yet available"}"#)
        .expect(1)
        .create_async()
        .await;
    let _second = server
        .mock("GET", "/api/v10/guilds/123/messages/search")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("content".into(), "hello".into()),
            Matcher::UrlEncoded("limit".into(), "10".into()),
            Matcher::UrlEncoded("offset".into(), "0".into()),
        ]))
        .with_status(202)
        .with_header("content-type", "application/json")
        .with_body(r#"{"code":110000,"message":"Index not yet available"}"#)
        .expect(1)
        .create_async()
        .await;

    let _token = EnvGuard::set("DISCORD_BOT_TOKEN", "token");
    let _guild = EnvGuard::set("DISCORD_GUILD_ID", "123");
    let tools = DiscordTools::with_config(agentic_config::types::DiscordServiceConfig {
        base_url: server.url(),
        request_timeout_secs: 5,
    });

    let err = tools
        .search_messages(DiscordSearchMessagesInput {
            query: "hello".into(),
            limit: None,
            offset: None,
            channel_id: None,
            author_id: None,
        })
        .await
        .unwrap_err();

    assert!(matches!(err, DiscordToolsError::IndexingInProgress));
    Ok(())
}
