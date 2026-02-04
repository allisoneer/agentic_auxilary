//! Integration tests for streaming download behavior in `web_fetch`.

use web_retrieval::WebTools;
use web_retrieval::fetch::{HARD_MAX_BYTES, web_fetch};
use web_retrieval::types::WebFetchInput;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn fetch_under_cap_is_not_truncated() {
    let server = MockServer::start().await;

    let body = vec![b'a'; 128];

    Mock::given(method("GET"))
        .and(path("/small"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "text/plain")
                .set_body_bytes(body.clone()),
        )
        .mount(&server)
        .await;

    let tools = WebTools::new();
    let input = WebFetchInput {
        url: format!("{}/small", server.uri()),
        summarize: false,
        max_bytes: Some(1024),
    };

    let out = web_fetch(&tools, input).await.unwrap();
    assert!(!out.truncated);
    assert_eq!(out.content.len(), body.len());
}

#[tokio::test]
async fn fetch_over_cap_is_truncated_and_returns_exactly_max_bytes() {
    let server = MockServer::start().await;

    let body = vec![b'a'; 2048];
    let cap = 512_usize;

    Mock::given(method("GET"))
        .and(path("/large"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "text/plain")
                .set_body_bytes(body),
        )
        .mount(&server)
        .await;

    let tools = WebTools::new();
    let input = WebFetchInput {
        url: format!("{}/large", server.uri()),
        summarize: false,
        max_bytes: Some(cap),
    };

    let out = web_fetch(&tools, input).await.unwrap();
    assert!(out.truncated);
    assert_eq!(out.content.len(), cap);
}

#[tokio::test]
async fn max_bytes_over_hard_cap_is_rejected_before_request() {
    let server = MockServer::start().await;

    let tools = WebTools::new();
    let input = WebFetchInput {
        url: format!("{}/never", server.uri()),
        summarize: false,
        max_bytes: Some(HARD_MAX_BYTES + 1),
    };

    let err = web_fetch(&tools, input).await.unwrap_err();
    assert!(err.to_string().contains("max_bytes"));

    let received = server.received_requests().await.unwrap();
    assert!(
        received.is_empty(),
        "request should not be sent on invalid max_bytes"
    );
}
