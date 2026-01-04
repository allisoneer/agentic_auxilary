//! Managed server lifecycle support.
//!
//! This module provides functionality to spawn and manage `opencode serve`.

use crate::error::{OpencodeError, Result};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use url::Url;

/// Options for starting a managed server.
#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// Port to listen on (None for random).
    pub port: Option<u16>,
    /// Hostname to bind to.
    pub hostname: String,
    /// Directory to run in.
    pub directory: Option<std::path::PathBuf>,
    /// Config JSON to inject via OPENCODE_CONFIG_CONTENT.
    pub config_json: Option<String>,
    /// Startup timeout in milliseconds (default: 5000).
    pub startup_timeout_ms: u64,
    /// Path to opencode binary.
    pub binary: String,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            port: None,
            hostname: "127.0.0.1".to_string(),
            directory: None,
            config_json: None,
            startup_timeout_ms: 5000,
            binary: "opencode".to_string(),
        }
    }
}

impl ServerOptions {
    /// Create a new ServerOptions with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the port.
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the hostname.
    pub fn hostname(mut self, hostname: impl Into<String>) -> Self {
        self.hostname = hostname.into();
        self
    }

    /// Set the directory.
    pub fn directory(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.directory = Some(dir.into());
        self
    }

    /// Set config JSON.
    pub fn config_json(mut self, json: impl Into<String>) -> Self {
        self.config_json = Some(json.into());
        self
    }

    /// Set startup timeout in milliseconds.
    pub fn startup_timeout_ms(mut self, ms: u64) -> Self {
        self.startup_timeout_ms = ms;
        self
    }

    /// Set the binary path.
    pub fn binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }
}

/// A managed OpenCode server instance.
///
/// The server is automatically stopped when this is dropped.
pub struct ManagedServer {
    /// Base URL of the running server.
    base_url: Url,
    /// The server child process.
    child: Child,
    /// Port the server is running on.
    port: u16,
}

impl ManagedServer {
    /// Start a new managed server.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start or doesn't become ready
    /// within the configured timeout.
    pub async fn start(opts: ServerOptions) -> Result<Self> {
        let port = opts
            .port
            .unwrap_or_else(|| portpicker::pick_unused_port().unwrap_or(4096));

        let mut cmd = Command::new(&opts.binary);
        cmd.arg("serve")
            .arg("--hostname")
            .arg(&opts.hostname)
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if let Some(dir) = &opts.directory {
            cmd.current_dir(dir);
        }
        if let Some(cfg) = &opts.config_json {
            cmd.env("OPENCODE_CONFIG_CONTENT", cfg);
        }

        let mut child = cmd.spawn().map_err(|e| OpencodeError::SpawnServer {
            message: e.to_string(),
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| OpencodeError::SpawnServer {
                message: "no stdout from server process".into(),
            })?;
        let mut reader = BufReader::new(stdout).lines();

        let base_url = Url::parse(&format!("http://{}:{}/", opts.hostname, port))
            .map_err(OpencodeError::Url)?;
        let start = Instant::now();
        let deadline = Duration::from_millis(opts.startup_timeout_ms);
        let ready_marker = "opencode server listening on";

        loop {
            if start.elapsed() > deadline {
                // Fallback: try /doc probe
                let probe_client = reqwest::Client::new();
                let doc_url = base_url.join("doc").unwrap();
                match probe_client
                    .get(doc_url)
                    .timeout(Duration::from_millis(500))
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => break,
                    _ => {
                        // Kill the child process
                        let _ = child.kill().await;
                        return Err(OpencodeError::ServerTimeout {
                            timeout_ms: opts.startup_timeout_ms,
                        });
                    }
                }
            }

            match tokio::time::timeout(Duration::from_millis(100), reader.next_line()).await {
                Ok(Ok(Some(line))) if line.contains(ready_marker) => break,
                Ok(Ok(Some(_))) => continue,
                Ok(Ok(None)) => {
                    return Err(OpencodeError::SpawnServer {
                        message: "Server process exited unexpectedly".into(),
                    });
                }
                Ok(Err(e)) => {
                    return Err(OpencodeError::SpawnServer {
                        message: format!("Error reading server output: {}", e),
                    });
                }
                Err(_) => continue, // timeout, keep trying
            }
        }

        Ok(Self {
            base_url,
            child,
            port,
        })
    }

    /// Get the base URL of the server.
    pub fn url(&self) -> &Url {
        &self.base_url
    }

    /// Get the port the server is running on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stop the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot be stopped.
    pub async fn stop(mut self) -> Result<()> {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        Ok(())
    }

    /// Check if the server is still running.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for ManagedServer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_options_defaults() {
        let opts = ServerOptions::default();
        assert!(opts.port.is_none());
        assert_eq!(opts.hostname, "127.0.0.1");
        assert_eq!(opts.startup_timeout_ms, 5000);
        assert_eq!(opts.binary, "opencode");
    }

    #[test]
    fn test_server_options_builder() {
        let opts = ServerOptions::new()
            .port(8080)
            .hostname("0.0.0.0")
            .startup_timeout_ms(10000)
            .binary("/usr/local/bin/opencode");

        assert_eq!(opts.port, Some(8080));
        assert_eq!(opts.hostname, "0.0.0.0");
        assert_eq!(opts.startup_timeout_ms, 10000);
        assert_eq!(opts.binary, "/usr/local/bin/opencode");
    }
}
