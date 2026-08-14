use std::collections::HashSet;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub allow_non_loopback: bool,
    pub allowed_origins: HashSet<String>,
    pub max_connections: usize,
    pub max_message_bytes: usize,
    pub max_json_depth: usize,
    pub max_json_nodes: usize,
    pub max_source_component_bytes: usize,
    pub max_source_order_bytes: usize,
    pub max_in_flight: usize,
    pub outbound_capacity: usize,
    pub publication_capacity: usize,
    pub replay_page_size: usize,
    pub max_delivery_claims: usize,
    pub max_delivery_text_bytes: usize,
    pub scheduler_poll_interval: Duration,
    pub scheduler_batch_size: usize,
    pub scheduler_error_backoff_max: Duration,
    pub shutdown_grace: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            allow_non_loopback: false,
            allowed_origins: HashSet::new(),
            max_connections: 128,
            max_message_bytes: 1_048_576,
            max_json_depth: 32,
            max_json_nodes: 8192,
            max_source_component_bytes: 256,
            max_source_order_bytes: 4096,
            max_in_flight: 32,
            outbound_capacity: 128,
            publication_capacity: 256,
            replay_page_size: 256,
            max_delivery_claims: 256,
            max_delivery_text_bytes: 65_536,
            scheduler_poll_interval: Duration::from_millis(250),
            scheduler_batch_size: 256,
            scheduler_error_backoff_max: Duration::from_secs(5),
            shutdown_grace: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("non-loopback bind requires allow_non_loopback")]
    NonLoopback,
    #[error("all configured limits must be non-zero and fit protocol fields")]
    InvalidLimit,
    #[error(
        "allowed origins must be absolute http(s) origins without path, query, fragment, or credentials"
    )]
    InvalidOrigin,
}

impl ServerConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.bind.ip().is_loopback() && !self.allow_non_loopback {
            return Err(ConfigError::NonLoopback);
        }
        if [
            self.max_connections,
            self.max_message_bytes,
            self.max_json_depth,
            self.max_json_nodes,
            self.max_source_component_bytes,
            self.max_source_order_bytes,
            self.max_in_flight,
            self.outbound_capacity,
            self.publication_capacity,
            self.replay_page_size,
            self.max_delivery_claims,
            self.max_delivery_text_bytes,
            self.scheduler_batch_size,
        ]
        .contains(&0)
            || u32::try_from(self.max_message_bytes).is_err()
            || u32::try_from(self.max_connections).is_err()
            || u32::try_from(self.max_in_flight).is_err()
            || self.scheduler_poll_interval.is_zero()
            || self.scheduler_error_backoff_max.is_zero()
            || self.shutdown_grace.is_zero()
        {
            return Err(ConfigError::InvalidLimit);
        }
        if self
            .allowed_origins
            .iter()
            .any(|origin| !valid_origin(origin))
        {
            return Err(ConfigError::InvalidOrigin);
        }
        Ok(())
    }
}

fn valid_origin(origin: &str) -> bool {
    let Some(rest) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    !rest.is_empty()
        && !rest.starts_with(':')
        && !rest.contains(['/', '?', '#', '@'])
        && !rest.chars().any(char::is_whitespace)
        && !origin.ends_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_require_a_nonempty_host_before_any_port() {
        for origin in ["http://:8080", "https://:443", "http://:"] {
            assert!(!valid_origin(origin), "origin {origin}");
        }
        for origin in ["http://localhost:8080", "https://example.com:443"] {
            assert!(valid_origin(origin), "origin {origin}");
        }
    }
}
