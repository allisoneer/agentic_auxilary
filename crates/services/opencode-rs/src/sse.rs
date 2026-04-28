//! SSE (Server-Sent Events) streaming support.
//!
//! This module provides SSE subscription with reconnection and backoff.

use crate::error::Result;
use crate::types::event::Event;
use crate::types::event::GlobalEvent;
use backon::BackoffBuilder;
use backon::ExponentialBuilder;
use futures::StreamExt;
use reqwest::Client as ReqClient;
use reqwest_eventsource::Event as EsEvent;
use reqwest_eventsource::EventSource;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
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
pub struct SseSubscription<T> {
    rx: mpsc::Receiver<T>,
    cancel: CancellationToken,
    _task: tokio::task::JoinHandle<()>,
}

impl<T> SseSubscription<T> {
    /// Receive the next event.
    ///
    /// Returns `None` if the stream is closed.
    pub async fn recv(&mut self) -> Option<T> {
        self.rx.recv().await
    }

    /// Close the subscription explicitly.
    pub fn close(&self) {
        self.cancel.cancel();
    }
}

impl<T> Drop for SseSubscription<T> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// SSE subscriber for `OpenCode` events.
#[derive(Clone)]
pub struct SseSubscriber {
    http: ReqClient,
    base_url: String,
    directory: Option<String>,
    workspace: Option<String>,
    last_event_id: Arc<RwLock<Option<String>>>,
}

impl SseSubscriber {
    // TODO(3): Accept optional ReqClient to allow connection pool sharing with HttpClient

    /// Create a new SSE subscriber.
    pub fn new(
        base_url: String,
        directory: Option<String>,
        workspace: Option<String>,
        last_event_id: Arc<RwLock<Option<String>>>,
    ) -> Self {
        Self {
            http: ReqClient::new(),
            base_url,
            directory,
            workspace,
            last_event_id,
        }
    }

    /// Subscribe to events, optionally filtered by session ID.
    ///
    /// `OpenCode`'s `/event` endpoint streams all events for the configured directory.
    /// If `session_id` is provided, events will be filtered client-side to only
    /// include events for that session.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub fn subscribe_session(
        &self,
        session_id: &str,
        opts: SseOptions,
    ) -> Result<SseSubscription<Event>> {
        let url = format!("{}/event", self.base_url);
        let session_id = session_id.to_string();
        self.subscribe_filtered(url, opts, move |event: &Event| {
            event.session_id() == Some(session_id.as_str())
        })
    }

    /// Subscribe to all events for the configured directory.
    ///
    /// This uses the `/event` endpoint which streams all events for the
    /// directory specified via query parameters.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub fn subscribe(&self, opts: SseOptions) -> Result<SseSubscription<Event>> {
        let url = format!("{}/event", self.base_url);
        self.subscribe_filtered(url, opts, |_| true)
    }

    /// Subscribe to global events (all directories).
    ///
    /// This uses the `/global/event` endpoint which streams events from all
    /// `OpenCode` instances across all directories. Events are wrapped in a
    /// `GlobalEvent` values with directory/workspace context.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub fn subscribe_global(&self, opts: SseOptions) -> Result<SseSubscription<GlobalEvent>> {
        let url = format!("{}/global/event", self.base_url);
        self.subscribe_filtered(url, opts, |_| true)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "API consistency with public methods"
    )]
    fn subscribe_filtered<T, F>(
        &self,
        url: String,
        opts: SseOptions,
        should_send: F,
    ) -> Result<SseSubscription<T>>
    where
        T: serde::de::DeserializeOwned + Send + 'static,
        F: Fn(&T) -> bool + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel(opts.capacity);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let http = self.http.clone();
        let dir = self.directory.clone();
        let workspace = self.workspace.clone();
        let lei = Arc::clone(&self.last_event_id);
        let initial = opts.initial_interval;
        let max = opts.max_interval;
        let should_send = Arc::new(should_send);

        let task = tokio::spawn(async move {
            // Note: No max_times means the subscriber will retry indefinitely.
            // This is intentional for long-lived SSE connections that should reconnect
            // on any transient network failure.
            let backoff_builder = ExponentialBuilder::default()
                .with_min_delay(initial)
                .with_max_delay(max)
                .with_factor(2.0)
                .with_jitter();

            let mut backoff = backoff_builder.build();

            loop {
                if cancel_clone.is_cancelled() {
                    break;
                }

                let mut req = http.get(&url);
                if !url.ends_with("/global/event") {
                    if let Some(d) = &dir {
                        req = req.query(&[("directory", d)]);
                    }
                    if let Some(ws) = &workspace {
                        req = req.query(&[("workspace", ws)]);
                    }
                }
                let last_id = lei.read().await.clone();
                if let Some(id) = last_id {
                    req = req.header("Last-Event-ID", id);
                }

                let es_result = EventSource::new(req);
                let mut es = match es_result {
                    Ok(es) => es,
                    Err(e) => {
                        tracing::warn!("Failed to create EventSource: {:?}", e);
                        if let Some(delay) = backoff.next() {
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
                            // Reset backoff on successful connection
                            backoff = backoff_builder.build();
                            tracing::debug!("SSE connection opened");
                        }
                        Ok(EsEvent::Message(msg)) => {
                            // Track last event ID
                            if !msg.id.is_empty() {
                                *lei.write().await = Some(msg.id.clone());
                            }

                            // Parse event
                            match serde_json::from_str::<T>(&msg.data) {
                                Ok(ev) => {
                                    if should_send.as_ref()(&ev) && tx.send(ev).await.is_err() {
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
                if let Some(delay) = backoff.next() {
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
            None,
            Arc::new(RwLock::new(None)),
        );

        // This will fail to connect but we can test cancellation
        let opts = SseOptions {
            capacity: 1,
            initial_interval: Duration::from_millis(10),
            max_interval: Duration::from_millis(50),
        };

        let subscription = subscriber.subscribe_global(opts).unwrap();
        subscription.close();
        // Subscription should be cancelled
        assert!(subscription.cancel.is_cancelled());
    }

    // ==================== Question Event Parsing Tests ====================

    #[test]
    fn test_question_asked_event_parsing() {
        let json = r#"{
            "type": "question.asked",
            "properties": {
                "id": "req-123",
                "sessionID": "sess-456",
                "questions": [
                    {"question": "Continue?", "header": "Confirm action"}
                ]
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::QuestionAsked { .. }));
        if let Event::QuestionAsked { properties } = &event {
            assert_eq!(properties.request.id, "req-123");
            assert_eq!(properties.request.session_id, "sess-456");
            assert_eq!(properties.request.questions.len(), 1);
        }
    }

    #[test]
    fn test_question_replied_event_parsing() {
        let json = r#"{
            "type": "question.replied",
            "properties": {
                "sessionID": "sess-456",
                "requestID": "req-123",
                "answers": [["Yes", "Confirm"], ["Option B"]]
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::QuestionReplied { .. }));
        if let Event::QuestionReplied { properties } = &event {
            assert_eq!(properties.session_id, "sess-456");
            assert_eq!(properties.request_id, "req-123");
            assert_eq!(properties.answers.len(), 2);
            assert_eq!(properties.answers[0], vec!["Yes", "Confirm"]);
        }
    }

    #[test]
    fn test_question_rejected_event_parsing() {
        let json = r#"{
            "type": "question.rejected",
            "properties": {
                "sessionID": "sess-456",
                "requestID": "req-123",
                "reason": "User cancelled the operation"
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::QuestionRejected { .. }));
        if let Event::QuestionRejected { properties } = &event {
            assert_eq!(properties.session_id, "sess-456");
            assert_eq!(properties.request_id, "req-123");
            assert_eq!(
                properties.reason,
                Some("User cancelled the operation".to_string())
            );
        }
    }

    #[test]
    fn test_question_rejected_event_without_reason() {
        let json = r#"{
            "type": "question.rejected",
            "properties": {
                "sessionID": "sess-456",
                "requestID": "req-123"
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        if let Event::QuestionRejected { properties } = &event {
            assert!(properties.reason.is_none());
        }
    }

    #[test]
    fn test_question_event_session_id_extraction() {
        // Test that session_id() method works for question events
        let asked_json = r#"{
            "type": "question.asked",
            "properties": {
                "id": "req-1",
                "sessionID": "sess-asked",
                "questions": []
            }
        }"#;
        let asked: Event = serde_json::from_str(asked_json).unwrap();
        assert_eq!(asked.session_id(), Some("sess-asked"));

        let replied_json = r#"{
            "type": "question.replied",
            "properties": {
                "sessionID": "sess-replied",
                "requestID": "req-1",
                "answers": []
            }
        }"#;
        let replied: Event = serde_json::from_str(replied_json).unwrap();
        assert_eq!(replied.session_id(), Some("sess-replied"));

        let rejected_json = r#"{
            "type": "question.rejected",
            "properties": {
                "sessionID": "sess-rejected",
                "requestID": "req-1"
            }
        }"#;
        let rejected: Event = serde_json::from_str(rejected_json).unwrap();
        assert_eq!(rejected.session_id(), Some("sess-rejected"));
    }
}
