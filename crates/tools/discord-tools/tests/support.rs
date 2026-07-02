use agentic_config::types::DiscordServiceConfig;
use discord_tools::DiscordTools;
use mockito::ServerGuard;
use std::sync::Once;

static RUSTLS_PROVIDER: Once = Once::new();

pub fn install_rustls_provider() {
    RUSTLS_PROVIDER.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

pub struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    #[must_use]
    pub fn set(key: &'static str, val: &str) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: These guards are only used from #[serial(env)] tests.
        unsafe { std::env::set_var(key, val) };
        Self { key, prev }
    }

    #[must_use]
    pub fn remove(key: &'static str) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: These guards are only used from #[serial(env)] tests.
        unsafe { std::env::remove_var(key) };
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(value) => {
                // SAFETY: These guards are only used from #[serial(env)] tests.
                unsafe { std::env::set_var(self.key, value) };
            }
            None => {
                // SAFETY: These guards are only used from #[serial(env)] tests.
                unsafe { std::env::remove_var(self.key) };
            }
        }
    }
}

pub struct DiscordTestSetup {
    pub server: ServerGuard,
    pub tools: DiscordTools,
    _token: EnvGuard,
    _guild: EnvGuard,
}

pub async fn setup_discord_tools() -> DiscordTestSetup {
    install_rustls_provider();
    let server = mockito::Server::new_async().await;
    let token = EnvGuard::set("DISCORD_BOT_TOKEN", "token");
    let guild = EnvGuard::set("DISCORD_GUILD_ID", "123");

    let tools = DiscordTools::with_config(DiscordServiceConfig {
        base_url: server.url(),
        request_timeout_secs: 5,
    });

    DiscordTestSetup {
        server,
        tools,
        _token: token,
        _guild: guild,
    }
}
