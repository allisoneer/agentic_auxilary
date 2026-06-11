use crate::http::HttpClient;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod agent;
pub mod command;
pub mod event;
pub mod health;
pub mod model;
pub mod permission;
pub mod provider;
pub mod question;
pub mod session;

#[derive(Clone)]
pub struct ApiClient {
    http: HttpClient,
    last_event_id: Arc<RwLock<Option<String>>>,
}

impl ApiClient {
    pub fn new(http: HttpClient, last_event_id: Arc<RwLock<Option<String>>>) -> Self {
        Self {
            http,
            last_event_id,
        }
    }

    pub fn event(&self) -> event::EventApi {
        event::EventApi::new(self.http.clone(), Arc::clone(&self.last_event_id))
    }

    pub fn agent(&self) -> agent::AgentApi {
        agent::AgentApi::new(self.http.clone())
    }

    pub fn command(&self) -> command::CommandApi {
        command::CommandApi::new(self.http.clone())
    }

    pub fn health(&self) -> health::HealthApi {
        health::HealthApi::new(self.http.clone())
    }

    pub fn model(&self) -> model::ModelApi {
        model::ModelApi::new(self.http.clone())
    }

    pub fn permission(&self) -> permission::PermissionApi {
        permission::PermissionApi::new(self.http.clone())
    }

    pub fn provider(&self) -> provider::ProviderApi {
        provider::ProviderApi::new(self.http.clone())
    }

    pub fn question(&self) -> question::QuestionApi {
        question::QuestionApi::new(self.http.clone())
    }

    pub fn session(&self) -> session::SessionApi {
        session::SessionApi::new(self.http.clone())
    }
}
