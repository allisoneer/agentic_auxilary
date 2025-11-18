use anthropic_async::{AnthropicConfig, config::ANTHROPIC_DEFAULT_BASE, test_support::EnvGuard};
use serial_test::serial;

#[test]
#[serial(env)]
fn test_anthropic_base_url_from_env() {
    let _g = EnvGuard::set("ANTHROPIC_BASE_URL", "https://custom.api.example.com");

    let config = AnthropicConfig::new();
    let base_url = config.api_base();

    assert_eq!(base_url, "https://custom.api.example.com");
}

#[test]
#[serial(env)]
fn test_anthropic_base_url_default_when_not_set() {
    let _g = EnvGuard::remove("ANTHROPIC_BASE_URL");

    let config = AnthropicConfig::new();
    let base_url = config.api_base();

    assert_eq!(base_url, ANTHROPIC_DEFAULT_BASE);
}

#[test]
#[serial(env)]
fn test_anthropic_base_url_override_with_builder() {
    let _g = EnvGuard::set("ANTHROPIC_BASE_URL", "https://env.example.com");

    // Builder should override env var
    let config = AnthropicConfig::new().with_api_base("https://builder.example.com");
    let base_url = config.api_base();

    assert_eq!(base_url, "https://builder.example.com");
}

#[test]
#[serial(env)]
fn test_api_key_from_env() {
    let _g1 = EnvGuard::set("ANTHROPIC_API_KEY", "test-key-123");
    let _g2 = EnvGuard::remove("ANTHROPIC_AUTH_TOKEN");

    let config = AnthropicConfig::new();

    // The config should have picked up the API key
    assert!(config.validate_auth().is_ok());
}

#[test]
#[serial(env)]
fn test_bearer_token_from_env() {
    let _g1 = EnvGuard::remove("ANTHROPIC_API_KEY");
    let _g2 = EnvGuard::set("ANTHROPIC_AUTH_TOKEN", "test-bearer-456");

    let config = AnthropicConfig::new();

    // The config should have picked up the bearer token
    assert!(config.validate_auth().is_ok());
}

#[test]
#[serial(env)]
fn test_both_auth_from_env() {
    let _g1 = EnvGuard::set("ANTHROPIC_API_KEY", "test-key-789");
    let _g2 = EnvGuard::set("ANTHROPIC_AUTH_TOKEN", "test-bearer-012");

    let config = AnthropicConfig::new();

    // The config should have picked up both
    assert!(config.validate_auth().is_ok());
}
