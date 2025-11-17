use anthropic_async::{AnthropicConfig, Client, config::BetaFeature};
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_beta_header_propagation() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header_exists("anthropic-beta"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [],
            "has_more": false,
            "first_id": null,
            "last_id": null
        })))
        .mount(&server)
        .await;

    let cfg = AnthropicConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test")
        .with_beta_features([
            BetaFeature::PromptCaching20240731,
            BetaFeature::TokenCounting20241101,
        ]);

    let client = Client::with_config(cfg);
    let _ = client.models().list(&()).await.unwrap();
}
