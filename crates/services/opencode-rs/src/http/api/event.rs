use crate::error::Result;
use crate::http::HttpClient;
use crate::sse::SseOptions;
use crate::sse::SseSubscriber;
use crate::sse::SseSubscription;
use crate::types::v2::event::Event;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct EventApi {
    http: HttpClient,
    last_event_id: Arc<RwLock<Option<String>>>,
}

impl EventApi {
    pub fn new(http: HttpClient, last_event_id: Arc<RwLock<Option<String>>>) -> Self {
        Self {
            http,
            last_event_id,
        }
    }

    pub fn subscribe(&self) -> Result<SseSubscription<Event>> {
        self.subscribe_with_options(SseOptions::default())
    }

    pub fn subscribe_with_options(&self, options: SseOptions) -> Result<SseSubscription<Event>> {
        SseSubscriber::new(
            self.http.base().to_string(),
            self.http.directory().map(std::string::ToString::to_string),
            self.http.workspace().map(std::string::ToString::to_string),
            self.http
                .auth_header()
                .map(std::string::ToString::to_string),
            Arc::clone(&self.last_event_id),
        )
        .subscribe_api("/api/event", options)
    }
}
