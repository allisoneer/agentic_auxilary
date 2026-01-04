//! SSE (Server-Sent Events) streaming support.
//!
//! This module provides SSE subscription with reconnection and backoff.

use crate::error::Result;
use crate::types::event::Event;
use backoff::ExponentialBackoff;
use backoff::backoff::Backoff;
use futures::StreamExt;
use reqwest::Client as ReqClient;
use reqwest_eventsource::{Event as EsEvent, EventSource};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

/// Options for SSE subscription.
#[derive(Clone, Copy, Debug)]
pub struct SseOptions {
    /// Channel capacity (default: 256).
    pub capacity: usize,
    /// Initial backoff interval (default: 250ms).
    pub initial_interval: Duration,
    /// Max backoff interval (default: 30s).
    pub max_interval: Duration,
}

impl Default for SseOptions {
    fn default() -> Self {
        Self {
            capacity: 256,
            initial_interval: Duration::from_millis(250),
            max_interval: Duration::from_secs(30),
        }
    }
}

/// Handle to an active SSE subscription.
///
/// Dropping this handle will cancel the subscription.
pub struct SseSubscription {
    rx: mpsc::Receiver<Event>,
    cancel: CancellationToken,
    _task: tokio::task::JoinHandle<()>,
}

impl SseSubscription {
    /// Receive the next event.
    ///
    /// Returns `None` if the stream is closed.
    pub async fn recv(&mut self) -> Option<Event> {
        self.rx.recv().await
    }

    /// Close the subscription explicitly.
    pub fn close(&self) {
        self.cancel.cancel();
    }
}

impl Drop for SseSubscription {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// SSE subscriber for OpenCode events.
#[derive(Clone)]
pub struct SseSubscriber {
    http: ReqClient,
    base_url: String,
    directory: Option<String>,
    last_event_id: Arc<RwLock<Option<String>>>,
}

impl SseSubscriber {
    // TODO(3): Accept optional ReqClient to allow connection pool sharing with HttpClient

    /// Create a new SSE subscriber.
    pub fn new(
        base_url: String,
        directory: Option<String>,
        last_event_id: Arc<RwLock<Option<String>>>,
    ) -> Self {
        Self {
            http: ReqClient::new(),
            base_url,
            directory,
            last_event_id,
        }
    }

    /// Subscribe to events, optionally filtered by session ID.
    ///
    /// OpenCode's `/event` endpoint streams all events for the configured directory.
    /// If `session_id` is provided, events will be filtered client-side to only
    /// include events for that session.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe_session(
        &self,
        session_id: &str,
        opts: SseOptions,
    ) -> Result<SseSubscription> {
        let url = format!("{}/event", self.base_url);
        self.subscribe_filtered(url, Some(session_id.to_string()), opts)
            .await
    }

    /// Subscribe to all events for the configured directory.
    ///
    /// This uses the `/event` endpoint which streams all events for the
    /// directory specified via the `x-opencode-directory` header.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe(&self, opts: SseOptions) -> Result<SseSubscription> {
        let url = format!("{}/event", self.base_url);
        self.subscribe_filtered(url, None, opts).await
    }

    /// Subscribe to global events (all directories).
    ///
    /// This uses the `/global/event` endpoint which streams events from all
    /// OpenCode instances across all directories. Events are wrapped in a
    /// `GlobalEventEnvelope` with directory context.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe_global(&self, opts: SseOptions) -> Result<SseSubscription> {
        let url = format!("{}/global/event", self.base_url);
        self.subscribe_filtered(url, None, opts).await
    }

    async fn subscribe_filtered(
        &self,
        url: String,
        session_filter: Option<String>,
        opts: SseOptions,
    ) -> Result<SseSubscription> {
        let (tx, rx) = mpsc::channel(opts.capacity);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let http = self.http.clone();
        let dir = self.directory.clone();
        let lei = self.last_event_id.clone();
        let initial = opts.initial_interval;
        let max = opts.max_interval;
        let filter = session_filter;

        let task = tokio::spawn(async move {
            // Note: max_elapsed_time: None means the subscriber will retry indefinitely.
            // This is intentional for long-lived SSE connections that should reconnect
            // on any transient network failure.
            let mut backoff = ExponentialBackoff {
                initial_interval: initial,
                max_interval: max,
                max_elapsed_time: None,
                ..ExponentialBackoff::default()
            };

            loop {
                if cancel_clone.is_cancelled() {
                    break;
                }

                let mut req = http.get(&url);
                if let Some(d) = &dir {
                    req = req.header("x-opencode-directory", d);
                }
                if let Some(id) = lei.read().await.clone() {
                    req = req.header("Last-Event-ID", id);
                }

                let es_result = EventSource::new(req);
                let mut es = match es_result {
                    Ok(es) => es,
                    Err(e) => {
                        tracing::warn!("Failed to create EventSource: {:?}", e);
                        if let Some(delay) = backoff.next_backoff() {
                            tokio::select! {
                                () = tokio::time::sleep(delay) => {}
                                () = cancel_clone.cancelled() => { return; }
                            }
                        }
                        continue;
                    }
                };

                while let Some(event) = es.next().await {
                    if cancel_clone.is_cancelled() {
                        es.close();
                        return;
                    }

                    match event {
                        Ok(EsEvent::Open) => {
                            backoff.reset();
                            tracing::debug!("SSE connection opened");
                        }
                        Ok(EsEvent::Message(msg)) => {
                            // Track last event ID
                            if !msg.id.is_empty() {
                                *lei.write().await = Some(msg.id.clone());
                            }

                            // Parse event
                            match serde_json::from_str::<Event>(&msg.data) {
                                Ok(ev) => {
                                    // Apply session filter if specified
                                    let should_send = match &filter {
                                        Some(sid) => ev.session_id() == Some(sid.as_str()),
                                        None => true,
                                    };

                                    if should_send && tx.send(ev).await.is_err() {
                                        es.close();
                                        return;
                                    }
                                }
                                Err(e) => {
                                    // TODO(3): Consider exposing parse errors via Error event variant or callback
                                    tracing::warn!("Failed to parse SSE event: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("SSE error: {:?}", e);
                            es.close();
                            break; // Break inner loop to reconnect
                        }
                    }
                }

                // Apply backoff before reconnecting
                if let Some(delay) = backoff.next_backoff() {
                    tracing::debug!("SSE reconnecting after {:?}", delay);
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {}
                        () = cancel_clone.cancelled() => { return; }
                    }
                }
            }
        });

        Ok(SseSubscription {
            rx,
            cancel,
            _task: task,
        })
    }
}

#[cfg(test)]
mod tests {
    // TODO(2): Add tests for session filtering logic (lines 216-219), Last-Event-ID
    // tracking/resume behavior (lines 208-210, 176-178), and backoff timing (with
    // tokio time mocking).
    use super::*;

    #[test]
    fn test_sse_options_defaults() {
        let opts = SseOptions::default();
        assert_eq!(opts.capacity, 256);
        assert_eq!(opts.initial_interval, Duration::from_millis(250));
        assert_eq!(opts.max_interval, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_subscription_cancel_on_close() {
        let subscriber = SseSubscriber::new(
            "http://localhost:9999".to_string(),
            None,
            Arc::new(RwLock::new(None)),
        );

        // This will fail to connect but we can test cancellation
        let opts = SseOptions {
            capacity: 1,
            initial_interval: Duration::from_millis(10),
            max_interval: Duration::from_millis(50),
        };

        let subscription = subscriber.subscribe_global(opts).await.unwrap();
        subscription.close();
        // Subscription should be cancelled
        assert!(subscription.cancel.is_cancelled());
    }
}
