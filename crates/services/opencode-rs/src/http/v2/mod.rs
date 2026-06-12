//! Parallel V2 HTTP client surface for `/api/*` endpoints.
//!
//! Intentionally deferred in this additive layer:
//! - `/api/fs/read/*` raw binary responses
//! - `/api/event` SSE streaming transport

use crate::error::OpencodeError;
use crate::error::Result;
use reqwest::Method;
use reqwest::Response;
use serde::de::DeserializeOwned;

pub mod connector;
pub mod fs;
pub mod health;
pub mod location;
pub mod message;
pub mod model;
pub mod permission;
pub mod provider;
pub mod question;
pub mod reference;
pub mod session;

#[derive(Clone)]
pub struct V2Client {
    http: V2HttpClient,
}

impl V2Client {
    pub(crate) fn new(legacy: super::HttpClient) -> Self {
        Self {
            http: V2HttpClient::from_legacy(legacy),
        }
    }

    pub fn http(&self) -> &V2HttpClient {
        &self.http
    }

    pub fn health(&self) -> health::HealthApi {
        health::HealthApi::new(self.http.clone())
    }

    pub fn connector(&self) -> connector::ConnectorApi {
        connector::ConnectorApi::new(self.http.clone())
    }

    pub fn fs(&self) -> fs::FsApi {
        fs::FsApi::new(self.http.clone())
    }

    pub fn location(&self) -> location::LocationApi {
        location::LocationApi::new(self.http.clone())
    }

    pub fn session(&self) -> session::SessionApi {
        session::SessionApi::new(self.http.clone())
    }

    pub fn message(&self) -> message::MessageApi {
        message::MessageApi::new(self.http.clone())
    }

    pub fn model(&self) -> model::ModelApi {
        model::ModelApi::new(self.http.clone())
    }

    pub fn provider(&self) -> provider::ProviderApi {
        provider::ProviderApi::new(self.http.clone())
    }

    pub fn permission(&self) -> permission::PermissionApi {
        permission::PermissionApi::new(self.http.clone())
    }

    pub fn question(&self) -> question::QuestionApi {
        question::QuestionApi::new(self.http.clone())
    }

    pub fn reference(&self) -> reference::ReferenceApi {
        reference::ReferenceApi::new(self.http.clone())
    }
}

#[derive(Clone)]
pub struct V2HttpClient {
    inner: reqwest::Client,
    base_url: String,
    directory: Option<String>,
    workspace: Option<String>,
}

impl V2HttpClient {
    fn from_legacy(legacy: super::HttpClient) -> Self {
        Self {
            inner: legacy.inner,
            base_url: legacy.cfg.base_url,
            directory: legacy.cfg.directory,
            workspace: legacy.cfg.workspace,
        }
    }

    fn build_request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.inner.request(method, &url);

        if let Some(directory) = &self.directory {
            req = req.query(&[("location[directory]", directory)]);
        }

        if let Some(workspace) = &self.workspace {
            req = req.query(&[("location[workspace]", workspace)]);
        }

        req
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .build_request(Method::GET, path)
            .send()
            .await
            .map_err(OpencodeError::from)?;
        Self::map_json_response(resp).await
    }

    pub async fn get_with_query<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let mut req = self.build_request(Method::GET, path);
        if !query.is_empty() {
            let query_pairs: Vec<(&str, &str)> = query
                .iter()
                .map(|(key, value)| (*key, value.as_str()))
                .collect();
            req = req.query(&query_pairs);
        }

        let resp = req.send().await.map_err(OpencodeError::from)?;
        Self::map_json_response(resp).await
    }

    pub async fn post<TReq: serde::Serialize + Sync, TRes: DeserializeOwned>(
        &self,
        path: &str,
        body: &TReq,
    ) -> Result<TRes> {
        let resp = self
            .build_request(Method::POST, path)
            .json(body)
            .send()
            .await
            .map_err(OpencodeError::from)?;
        Self::map_json_response(resp).await
    }

    pub async fn post_empty<TReq: serde::Serialize + Sync>(
        &self,
        path: &str,
        body: &TReq,
    ) -> Result<()> {
        let resp = self
            .build_request(Method::POST, path)
            .json(body)
            .send()
            .await
            .map_err(OpencodeError::from)?;
        Self::check_status(resp).await
    }

    pub async fn delete_empty(&self, path: &str) -> Result<()> {
        let resp = self
            .build_request(Method::DELETE, path)
            .send()
            .await
            .map_err(OpencodeError::from)?;
        Self::check_status(resp).await
    }

    async fn map_json_response<T: DeserializeOwned>(resp: Response) -> Result<T> {
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(OpencodeError::from)?;

        if !status.is_success() {
            let body_text = String::from_utf8_lossy(&bytes);
            return Err(OpencodeError::http(status.as_u16(), &body_text));
        }

        serde_json::from_slice(&bytes).map_err(OpencodeError::from)
    }

    async fn check_status(resp: Response) -> Result<()> {
        let status = resp.status();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(OpencodeError::http(status.as_u16(), &body));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpClient;
    use crate::http::HttpConfig;
    use std::time::Duration;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;
    use wiremock::matchers::query_param;

    #[tokio::test]
    async fn v2_requests_use_location_query_without_legacy_query_keys() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/location"))
            .and(query_param("location[directory]", "/tmp/project"))
            .and(query_param("location[workspace]", "workspace-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock_server)
            .await;

        let legacy = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: Some("/tmp/project".to_string()),
            workspace: Some("workspace-1".to_string()),
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let client = V2Client::new(legacy);
        let _: serde_json::Value = client.http().get("/api/location").await.unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        let url = &requests[0].url;
        let query = url.query().unwrap_or_default();
        assert!(query.contains("location%5Bdirectory%5D=%2Ftmp%2Fproject"));
        assert!(query.contains("location%5Bworkspace%5D=workspace-1"));
        assert!(!query.contains("directory=%2Ftmp%2Fproject"));
        assert!(!query.contains("workspace=workspace-1"));
    }

    #[tokio::test]
    async fn v2_requests_preserve_additional_query_parameters() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/session"))
            .and(query_param("location[directory]", "/tmp/project"))
            .and(query_param("cursor", "next-page"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock_server)
            .await;

        let legacy = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: Some("/tmp/project".to_string()),
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let client = V2Client::new(legacy);
        let _: serde_json::Value = client
            .http()
            .get_with_query("/api/session", &[("cursor", "next-page".to_string())])
            .await
            .unwrap();
    }
}
