#![expect(clippy::expect_used, clippy::unwrap_used)]

use opencode_rs::ClientBuilder;
use opencode_rs::OpencodeError;
use opencode_rs::sse::SseOptions;
use opencode_rs::types::event::Event;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio::time::timeout;

const FIXTURE_TIMEOUT: Duration = Duration::from_secs(2);
const PENDING_CHECK: Duration = Duration::from_millis(25);
const SSE_HEADERS: &str =
    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";

async fn read_request(stream: &mut TcpStream) {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = timeout(FIXTURE_TIMEOUT, stream.read(&mut chunk))
            .await
            .expect("request read timed out")
            .expect("request read failed");
        assert!(read > 0, "connection closed before request headers");
        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            return;
        }
    }
}

async fn test_listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    (listener, base_url)
}

fn options() -> SseOptions {
    SseOptions {
        capacity: 8,
        initial_interval: Duration::from_millis(5),
        max_interval: Duration::from_millis(10),
    }
}

fn client(base_url: &str) -> opencode_rs::Client {
    ClientBuilder::new().base_url(base_url).build().unwrap()
}

fn session_idle(session_id: &str) -> String {
    format!(
        "data: {{\"type\":\"session.idle\",\"properties\":{{\"sessionID\":\"{session_id}\"}}}}\n\n"
    )
}

#[tokio::test]
async fn readiness_waits_for_headers_is_repeatable_and_preserves_filtering() {
    let (listener, base_url) = test_listener().await;
    let (request_tx, request_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = timeout(FIXTURE_TIMEOUT, listener.accept())
            .await
            .expect("accept timed out")
            .expect("accept failed");
        read_request(&mut stream).await;
        request_tx.send(()).expect("request receiver dropped");
        timeout(FIXTURE_TIMEOUT, release_rx)
            .await
            .expect("header release timed out")
            .expect("header release sender dropped");
        stream.write_all(SSE_HEADERS.as_bytes()).await.unwrap();
        stream
            .write_all(session_idle("other-session").as_bytes())
            .await
            .unwrap();
        stream
            .write_all(session_idle("target-session").as_bytes())
            .await
            .unwrap();
        stream.shutdown().await.unwrap();
    });

    let mut subscription = client(&base_url)
        .sse_subscriber()
        .subscribe_session("target-session", options())
        .unwrap();
    timeout(FIXTURE_TIMEOUT, request_rx)
        .await
        .expect("SSE request did not arrive")
        .expect("request signal sender dropped");

    assert!(
        timeout(PENDING_CHECK, subscription.wait_for_initial_connection())
            .await
            .is_err(),
        "readiness completed before response headers"
    );

    release_tx.send(()).expect("fixture server dropped");
    timeout(FIXTURE_TIMEOUT, subscription.wait_for_initial_connection())
        .await
        .expect("readiness timed out")
        .expect("readiness failed");
    timeout(PENDING_CHECK, subscription.wait_for_initial_connection())
        .await
        .expect("repeat readiness should return immediately")
        .expect("repeat readiness failed");

    let event = timeout(FIXTURE_TIMEOUT, subscription.recv())
        .await
        .expect("filtered event timed out")
        .expect("event stream closed");
    assert!(matches!(
        event,
        Event::SessionIdle { properties } if properties.session_id == "target-session"
    ));
    timeout(FIXTURE_TIMEOUT, server)
        .await
        .expect("fixture server did not terminate")
        .expect("fixture server panicked");
}

#[tokio::test]
async fn readiness_ignores_failed_attempts_before_open() {
    let (listener, base_url) = test_listener().await;
    let (second_request_tx, second_request_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut first, _) = timeout(FIXTURE_TIMEOUT, listener.accept())
            .await
            .expect("first accept timed out")
            .expect("first accept failed");
        read_request(&mut first).await;
        first.shutdown().await.unwrap();

        let (mut second, _) = timeout(FIXTURE_TIMEOUT, listener.accept())
            .await
            .expect("second accept timed out")
            .expect("second accept failed");
        read_request(&mut second).await;
        second_request_tx
            .send(())
            .expect("second request receiver dropped");
        timeout(FIXTURE_TIMEOUT, release_rx)
            .await
            .expect("second header release timed out")
            .expect("second header release sender dropped");
        second.write_all(SSE_HEADERS.as_bytes()).await.unwrap();
        second.shutdown().await.unwrap();
    });

    let mut subscription = client(&base_url)
        .sse_subscriber()
        .subscribe(options())
        .unwrap();
    timeout(FIXTURE_TIMEOUT, second_request_rx)
        .await
        .expect("second SSE request did not arrive")
        .expect("second request signal sender dropped");
    assert!(
        timeout(PENDING_CHECK, subscription.wait_for_initial_connection())
            .await
            .is_err(),
        "failed connection attempt marked readiness"
    );

    release_tx.send(()).expect("fixture server dropped");
    timeout(FIXTURE_TIMEOUT, subscription.wait_for_initial_connection())
        .await
        .expect("readiness timed out after successful retry")
        .expect("readiness failed after successful retry");
    subscription.close();
    timeout(FIXTURE_TIMEOUT, server)
        .await
        .expect("fixture server did not terminate")
        .expect("fixture server panicked");
}

#[tokio::test]
async fn readiness_stays_true_during_reconnect() {
    let (listener, base_url) = test_listener().await;
    let (second_request_tx, second_request_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut first, _) = timeout(FIXTURE_TIMEOUT, listener.accept())
            .await
            .expect("first accept timed out")
            .expect("first accept failed");
        read_request(&mut first).await;
        first.write_all(SSE_HEADERS.as_bytes()).await.unwrap();
        first.shutdown().await.unwrap();

        let (mut second, _) = timeout(FIXTURE_TIMEOUT, listener.accept())
            .await
            .expect("reconnect accept timed out")
            .expect("reconnect accept failed");
        read_request(&mut second).await;
        second_request_tx
            .send(())
            .expect("reconnect request receiver dropped");
        timeout(FIXTURE_TIMEOUT, release_rx)
            .await
            .expect("reconnect release timed out")
            .expect("reconnect release sender dropped");
        second.write_all(SSE_HEADERS.as_bytes()).await.unwrap();
        second.shutdown().await.unwrap();
    });

    let mut subscription = client(&base_url)
        .sse_subscriber()
        .subscribe(options())
        .unwrap();
    timeout(FIXTURE_TIMEOUT, subscription.wait_for_initial_connection())
        .await
        .expect("initial readiness timed out")
        .expect("initial readiness failed");
    timeout(FIXTURE_TIMEOUT, second_request_rx)
        .await
        .expect("reconnect request did not arrive")
        .expect("reconnect request signal sender dropped");

    timeout(PENDING_CHECK, subscription.wait_for_initial_connection())
        .await
        .expect("sticky readiness blocked during reconnect")
        .expect("sticky readiness failed during reconnect");
    release_tx.send(()).expect("fixture server dropped");
    subscription.close();
    timeout(FIXTURE_TIMEOUT, server)
        .await
        .expect("fixture server did not terminate")
        .expect("fixture server panicked");
}

#[tokio::test]
async fn close_interrupts_pending_initial_eventsource_read() {
    let (listener, base_url) = test_listener().await;
    let (request_tx, request_rx) = oneshot::channel();
    let (closed_tx, closed_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = timeout(FIXTURE_TIMEOUT, listener.accept())
            .await
            .expect("accept timed out")
            .expect("accept failed");
        read_request(&mut stream).await;
        request_tx.send(()).expect("request receiver dropped");
        let mut byte = [0_u8; 1];
        let read = timeout(FIXTURE_TIMEOUT, stream.read(&mut byte))
            .await
            .expect("socket did not close after cancellation")
            .expect("socket close read failed");
        assert_eq!(read, 0, "expected peer EOF after cancellation");
        closed_tx.send(()).expect("closed receiver dropped");
    });

    let mut subscription = client(&base_url)
        .sse_subscriber()
        .subscribe(options())
        .unwrap();
    timeout(FIXTURE_TIMEOUT, request_rx)
        .await
        .expect("SSE request did not arrive")
        .expect("request signal sender dropped");
    subscription.close();

    let error = timeout(FIXTURE_TIMEOUT, subscription.wait_for_initial_connection())
        .await
        .expect("readiness did not terminate after close")
        .expect_err("readiness unexpectedly succeeded after close");
    assert!(matches!(error, OpencodeError::StreamClosed));
    timeout(FIXTURE_TIMEOUT, closed_rx)
        .await
        .expect("fixture did not observe socket closure")
        .expect("socket closure sender dropped");
    timeout(FIXTURE_TIMEOUT, server)
        .await
        .expect("fixture server did not terminate")
        .expect("fixture server panicked");
}
