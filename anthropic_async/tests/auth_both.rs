use anthropic_async::{AnthropicConfig, Client};
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_both_auth_sends_both_headers() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header_exists("x-api-key"))
        .and(header("authorization", "Bearer t123"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"data":[]}"#))
        .mount(&server)
        .await;

    let cfg = AnthropicConfig::new()
        .with_api_base(server.uri())
        .with_both("k123", "t123");

    let client = Client::with_config(cfg);
    let _ = client.models().list(&()).await.unwrap();
}
