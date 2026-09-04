#![allow(clippy::expect_used)]

use futures::FutureExt;
use opencode_rs::Client;
use opencode_rs::OpencodeError;
use opencode_rs::types::Event;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio::time::timeout;

const DEADLOCK_GUARD: Duration = Duration::from_secs(5);

enum StreamCommand {
    Send {
        data: String,
        flushed: oneshot::Sender<()>,
    },
    Close {
        closed: oneshot::Sender<()>,
    },
}

struct RawSseServer {
    base_url: String,
    connected: watch::Receiver<bool>,
    commands: mpsc::UnboundedSender<StreamCommand>,
    task: tokio::task::JoinHandle<()>,
}

impl RawSseServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("raw SSE listener should bind");
        let address = listener
            .local_addr()
            .expect("raw SSE listener should have an address");
        let (connected_tx, connected) = watch::channel(false);
        let (commands, mut command_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("SSE client should connect");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream
                    .read(&mut chunk)
                    .await
                    .expect("request headers should be readable");
                assert!(read > 0, "client closed before sending request headers");
                request.extend_from_slice(&chunk[..read]);
            }

            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n",
                )
                .await
                .expect("SSE response headers should be writable");
            stream.flush().await.expect("SSE headers should flush");
            connected_tx.send_replace(true);

            while let Some(command) = command_rx.recv().await {
                match command {
                    StreamCommand::Send { data, flushed } => {
                        stream
                            .write_all(format!("data: {data}\n\n").as_bytes())
                            .await
                            .expect("SSE event should be writable");
                        stream.flush().await.expect("SSE event should flush");
                        let _ = flushed.send(());
                    }
                    StreamCommand::Close { closed } => {
                        stream.shutdown().await.expect("SSE stream should close");
                        let _ = closed.send(());
                        break;
                    }
                }
            }
        });

        Self {
            base_url: format!("http://{address}"),
            connected,
            commands,
            task,
        }
    }

    fn client(&self) -> Client {
        Client::builder()
            .base_url(&self.base_url)
            .directory("/tmp")
            .build()
            .expect("test client should build")
    }

    async fn wait_connected(&mut self) {
        timeout(DEADLOCK_GUARD, async {
            while !*self.connected.borrow() {
                self.connected
                    .changed()
                    .await
                    .expect("server connection signal should remain open");
            }
        })
        .await
        .expect("SSE client should establish the HTTP stream");
    }

    async fn send_raw(&self, data: &str) {
        let (flushed, flushed_rx) = oneshot::channel();
        self.commands
            .send(StreamCommand::Send {
                data: data.to_string(),
                flushed,
            })
            .expect("SSE server task should remain active");
        timeout(DEADLOCK_GUARD, flushed_rx)
            .await
            .expect("SSE event flush should not deadlock")
            .expect("SSE event flush acknowledgment should arrive");
    }

    async fn close_stream(&self) {
        let (closed, closed_rx) = oneshot::channel();
        self.commands
            .send(StreamCommand::Close { closed })
            .expect("SSE server task should remain active");
        timeout(DEADLOCK_GUARD, closed_rx)
            .await
            .expect("SSE close should not deadlock")
            .expect("SSE close acknowledgment should arrive");
    }
}

impl Drop for RawSseServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn readiness_is_parsed_latched_and_precedes_session_filtering() {
    let mut server = RawSseServer::start().await;
    let mut subscription = server
        .client()
        .subscribe_session("session-1")
        .expect("session subscription should be created");

    server.wait_connected().await;
    assert!(
        subscription.wait_ready().now_or_never().is_none(),
        "HTTP connection and headers alone must not establish readiness"
    );

    server.send_raw(r#"{"type":"server.connected""#).await;
    assert!(
        subscription.wait_ready().now_or_never().is_none(),
        "malformed sentinel JSON must not establish readiness"
    );

    server
        .send_raw(r#"{"type":"server.connected","properties":{}}"#)
        .await;
    timeout(DEADLOCK_GUARD, subscription.wait_ready())
        .await
        .expect("parsed sentinel should release readiness")
        .expect("readiness should succeed");
    assert!(
        subscription.wait_ready().now_or_never().is_some(),
        "repeated readiness waits must complete immediately"
    );
    assert!(
        subscription.recv().now_or_never().is_none(),
        "session receive must continue filtering server.connected"
    );

    server
        .send_raw(r#"{"type":"session.idle","properties":{"sessionID":"session-1"}}"#)
        .await;
    let event = timeout(DEADLOCK_GUARD, subscription.recv())
        .await
        .expect("session event should be delivered")
        .expect("session stream should remain open");
    assert!(matches!(event, Event::SessionIdle { .. }));
}

#[tokio::test]
async fn cancellation_before_readiness_returns_stream_closed() {
    let mut server = RawSseServer::start().await;
    let mut subscription = server
        .client()
        .subscribe_session("session-1")
        .expect("session subscription should be created");

    server.wait_connected().await;
    subscription.close();
    server.close_stream().await;

    let error = timeout(DEADLOCK_GUARD, subscription.wait_ready())
        .await
        .expect("cancelled readiness should not deadlock")
        .expect_err("cancelled worker must fail readiness");
    assert!(matches!(error, OpencodeError::StreamClosed));
}
