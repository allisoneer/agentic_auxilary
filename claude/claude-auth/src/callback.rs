//! OAuth callback server for handling redirects

use crate::error::AuthError;
use http_body_util::Full;
use hyper::{body::Bytes, server::conn::http1, service::service_fn, Method, Request, Response};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use tokio::{net::TcpListener, sync::oneshot, time::{timeout, Duration}};
use url::Url;

const CALLBACK_PORT: u16 = 19876;

/// Run a local HTTP server to capture the OAuth callback with state validation
pub async fn run_callback_server(expected_state: &str) -> Result<String, AuthError> {
    let addr: SocketAddr = ([127, 0, 0, 1], CALLBACK_PORT).into();
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| AuthError::Network(format!("Failed to bind callback server: {e}")))?;

    let (tx, rx) = oneshot::channel::<String>();
    let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    let expected = expected_state.to_string();

    let (stream, _) = listener
        .accept()
        .await
        .map_err(|e| AuthError::Network(e.to_string()))?;

    let io = TokioIo::new(stream);
    let tx_clone = tx.clone();

    let service = service_fn(move |req: Request<hyper::body::Incoming>| {
        let tx = tx_clone.clone();
        let expected = expected.clone();
        async move {
            if req.method() == Method::GET && req.uri().path() == "/callback" {
                let full_url = format!("http://localhost:{}{}", CALLBACK_PORT, req.uri());
                let parsed = Url::parse(&full_url).ok();
                
                let code = parsed.as_ref().and_then(|u| 
                    u.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.to_string()));
                let state = parsed.as_ref().and_then(|u| 
                    u.query_pairs().find(|(k, _)| k == "state").map(|(_, v)| v.to_string()));

                // Validate state parameter is present
                if state.is_none() {
                    let html = r#"<!DOCTYPE html><html><body><h1>Authorization failed</h1><p>Missing state parameter</p></body></html>"#;
                    return Ok::<_, hyper::Error>(Response::builder()
                        .status(400)
                        .body(Full::new(Bytes::from(html)))
                        .unwrap());
                }
                
                // Validate state parameter matches expected
                if state.as_deref() != Some(expected.as_str()) {
                    let html = r#"<!DOCTYPE html><html><body><h1>Authorization failed</h1><p>Invalid state parameter</p></body></html>"#;
                    return Ok::<_, hyper::Error>(Response::builder()
                        .status(400)
                        .body(Full::new(Bytes::from(html)))
                        .unwrap());
                }
                
                // Send code if valid
                if let (Some(code), Some(sender)) = (code, tx.lock().unwrap().take()) {
                    let _ = sender.send(code);
                }

                let html = r#"<!DOCTYPE html><html><body>
                    <h1>Authentication successful!</h1>
                    <p>You can close this tab.</p>
                    <script>setTimeout(() => window.close(), 2000);</script>
                </body></html>"#;

                Ok::<_, hyper::Error>(Response::new(Full::new(Bytes::from(html))))
            } else {
                Ok(Response::builder()
                    .status(404)
                    .body(Full::new(Bytes::from("Not Found")))
                    .unwrap())
            }
        }
    });

    tokio::spawn(async move {
        let _ = http1::Builder::new().serve_connection(io, service).await;
    });

    // 5-minute timeout
    match timeout(Duration::from_secs(300), rx).await {
        Ok(Ok(code)) => Ok(code),
        Ok(Err(_)) => Err(AuthError::Other("Callback cancelled".into())),
        Err(_) => Err(AuthError::Other("OAuth callback timeout (5 minutes)".into())),
    }
}
