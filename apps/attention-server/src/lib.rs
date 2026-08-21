mod config;
mod connection;
pub mod mapping;
mod publication;
mod router;
pub mod runtime;
pub mod scheduler;
mod service;
mod state;
mod time;
mod turso_service;

pub use config::ConfigError;
pub use config::ServerConfig;
pub use publication::PublicationHub;
pub use publication::PublicationSubscription;
pub use publication::PublishOutcome;
pub use router::router;
pub use runtime::RuntimeError;
pub use runtime::RuntimeHandle;
pub use scheduler::Clock;
pub use scheduler::Sleeper;
pub use scheduler::SystemClock;
pub use service::AttentionMutationService;
pub use service::AttentionService;
pub use service::DeliveryPortService;
pub use service::DeliveryWorkerService;
pub use service::ReadPortService;
pub use service::ServiceError;
pub use service::SharedDeliveryWorkerService;
pub use service::SharedMutationService;
pub use service::SharedService;
pub use state::AppState;
pub use state::ServerIdentity;
pub use turso_service::TursoAttentionService;
#[cfg(feature = "test-support")]
pub use turso_service::fail_after_commit_before_publication_once;

use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpListener;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("server I/O failed: {0}")]
    Io(#[from] std::io::Error),
}
pub async fn serve(listener: TcpListener, state: Arc<AppState>) -> Result<(), ServerError> {
    let shutdown = state.shutdown.clone();
    let grace = state.config.shutdown_grace;
    let graceful = shutdown.clone();
    let server = async move {
        axum::serve(listener, router(state))
            .with_graceful_shutdown(graceful.cancelled_owned())
            .await
    };
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result?,
        () = shutdown.cancelled() => if let Ok(result) = tokio::time::timeout(grace, &mut server).await {
            result?;
        } else {
            tracing::warn!("server shutdown grace elapsed; forcing listener termination");
        },
    }
    Ok(())
}
