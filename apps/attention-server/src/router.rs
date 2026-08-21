use crate::AppState;
use crate::connection;
use axum::Router;
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/ws", get(upgrade))
        .with_state(state)
}
async fn upgrade(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    if let Some(origin) = headers.get("origin") {
        let origin = origin.to_str().map_err(|_| StatusCode::FORBIDDEN)?;
        if !state.config.allowed_origins.contains(origin) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    let permit = Arc::clone(&state.connection_slots)
        .try_acquire_owned()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let ws = ws
        .max_message_size(state.config.max_message_bytes)
        .max_frame_size(state.config.max_message_bytes);
    Ok(ws.on_upgrade(move |socket| connection::run(socket, state, permit)))
}
