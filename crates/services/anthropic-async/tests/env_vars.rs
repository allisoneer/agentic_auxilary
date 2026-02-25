use anthropic_async::{AnthropicConfig, config::ANTHROPIC_DEFAULT_BASE, test_support::EnvGuard};
use serial_test::serial;

#[test]
#[serial(env)]
fn test_anthropic_base_url_from_env() {
    EnvGuard::with_set(
        "ANTHROPIC_BASE_URL",
        "https://custom.api.example.com",
        || {
            let config = AnthropicConfig::new();
            assert_eq!(config.api_base(), "https://custom.api.example.com");
        },
    );
}

#[test]
#[serial(env)]
fn test_anthropic_base_url_default_when_not_set() {
    EnvGuard::with_removed("ANTHROPIC_BASE_URL", || {
        let config = AnthropicConfig::new();
        assert_eq!(config.api_base(), ANTHROPIC_DEFAULT_BASE);
    });
}

#[test]
#[serial(env)]
fn test_anthropic_base_url_override_with_builder() {
    EnvGuard::with_set("ANTHROPIC_BASE_URL", "https://env.example.com", || {
        // Builder should override env var
        let config = AnthropicConfig::new().with_api_base("https://builder.example.com");
        assert_eq!(config.api_base(), "https://builder.example.com");
    });
}

#[test]
#[serial(env)]
fn test_api_key_from_env() {
    EnvGuard::with_set("ANTHROPIC_API_KEY", "test-key-123", || {
        EnvGuard::with_removed("ANTHROPIC_AUTH_TOKEN", || {
            let config = AnthropicConfig::new();
            assert!(config.validate_auth().is_ok());
        });
    });
}

#[test]
#[serial(env)]
fn test_bearer_token_from_env() {
    EnvGuard::with_removed("ANTHROPIC_API_KEY", || {
        EnvGuard::with_set("ANTHROPIC_AUTH_TOKEN", "test-bearer-456", || {
            let config = AnthropicConfig::new();
            assert!(config.validate_auth().is_ok());
        });
    });
}

#[test]
#[serial(env)]
fn test_both_auth_from_env() {
    EnvGuard::with_set("ANTHROPIC_API_KEY", "test-key-789", || {
        EnvGuard::with_set("ANTHROPIC_AUTH_TOKEN", "test-bearer-012", || {
            let config = AnthropicConfig::new();
            assert!(config.validate_auth().is_ok());
        });
    });
}
