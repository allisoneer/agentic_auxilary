// TODO(3): clean up unwrap_used as part of broader gpt5_reasoner lint conformance pass.
#![allow(clippy::unwrap_used)]

use async_openai::types::chat::ChatCompletionRequestMessage;
use async_openai::types::chat::ChatCompletionRequestUserMessageArgs;
use async_openai::types::chat::CreateChatCompletionRequestArgs;
use serial_test::serial;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

struct EnvVarGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: This guard is used only from `#[serial(env)]` tests.
        unsafe { std::env::set_var(key, value) };
        Self { key, prev }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(prev) => {
                // SAFETY: This guard is used only from `#[serial(env)]` tests.
                unsafe { std::env::set_var(self.key, prev) }
            }
            None => {
                // SAFETY: This guard is used only from `#[serial(env)]` tests.
                unsafe { std::env::remove_var(self.key) }
            }
        }
    }
}

#[tokio::test]
#[serial(env)]
async fn orclient_honors_api_base_url() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{
              "id":"chatcmpl-1",
              "object":"chat.completion",
              "created":0,
              "model":"test",
              "choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]
            }"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let _api_key_guard = EnvVarGuard::set("OPENROUTER_API_KEY", "test");

    let client =
        gpt5_reasoner::client::OrClient::from_env(Some(&format!("{}/api/v1", server.uri())))
            .unwrap();

    let user_msg = ChatCompletionRequestUserMessageArgs::default()
        .content("hi")
        .build()
        .unwrap();

    let req = CreateChatCompletionRequestArgs::default()
        .model("test-model")
        .messages([ChatCompletionRequestMessage::User(user_msg)])
        .build()
        .unwrap();

    let resp = client.client.chat().create(req).await.unwrap();
    assert_eq!(resp.choices[0].message.content.as_deref(), Some("ok"));

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
}
