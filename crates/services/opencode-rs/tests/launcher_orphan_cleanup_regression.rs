#![cfg(unix)]
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[cfg(feature = "server")]
mod tests {
    use opencode_rs::server::ManagedServer;
    use opencode_rs::server::ServerOptions;
    use opencode_rs::version::PINNED_OPENCODE_VERSION;
    use std::process::Command;
    use std::time::Duration;
    use std::time::Instant;
    use tokio::net::TcpListener;

    fn should_run() -> bool {
        std::env::var("OPENCODE_INTEGRATION").is_ok()
    }

    fn pgrep_port(port: u16) -> String {
        let output = Command::new("pgrep")
            .args(["-af", "opencode"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.contains(&format!("--port {port}")))
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if predicate() {
                return;
            }

            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        panic!("condition not met within {timeout:?}");
    }

    #[tokio::test]
    #[ignore = "requires bunx + opencode-ai package; set OPENCODE_INTEGRATION=1"]
    async fn bunx_stop_kills_descendants_and_frees_port() {
        if !should_run() {
            return;
        }

        let server = ManagedServer::start(
            ServerOptions::default()
                .binary("bunx")
                .launcher_args(vec![
                    "--yes".to_string(),
                    format!("opencode-ai@{PINNED_OPENCODE_VERSION}"),
                ])
                .startup_timeout_ms(30_000),
        )
        .await
        .expect("start managed server");
        let port = server.port();

        let before_stop = pgrep_port(port);
        assert!(
            !before_stop.trim().is_empty(),
            "expected processes for --port {port}, got empty"
        );

        server.stop().await.expect("stop managed server");

        wait_until(Duration::from_secs(5), || {
            pgrep_port(port).trim().is_empty()
        })
        .await;
        wait_until(Duration::from_secs(5), || {
            std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
        })
        .await;

        let _listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("port should be bindable");
    }
}
