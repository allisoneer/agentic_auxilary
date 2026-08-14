use crate::AppState;
use crate::Clock;
use crate::PublicationHub;
use crate::ServerConfig;
use crate::ServerError;
use crate::ServerIdentity;
use crate::Sleeper;
use crate::SystemClock;
use crate::TursoAttentionService;
use crate::serve;
use attention_protocol as protocol;
use attention_turso::AttentionDatabase;
use attention_turso::Config as TursoConfig;
use attention_turso::Error as TursoError;
use attention_turso::PersistentServerIdentity;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("database startup failed")]
    Database(#[from] TursoError),
    #[error("server configuration failed")]
    Config(#[from] crate::ConfigError),
    #[error("listener failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("server failed: {0}")]
    Server(#[from] ServerError),
    #[error("runtime task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Running production server. Dropping this value requests shutdown; call
/// [`Self::shutdown`] to prove all upgraded connections and database work have stopped.
pub struct RuntimeHandle {
    address: SocketAddr,
    identity: ServerIdentity,
    shutdown: CancellationToken,
    task: JoinHandle<Result<(), RuntimeError>>,
}

impl RuntimeHandle {
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    pub const fn identity(&self) -> &ServerIdentity {
        &self.identity
    }

    pub async fn shutdown(mut self) -> Result<(), RuntimeError> {
        self.shutdown.cancel();
        (&mut self.task).await?
    }
}

impl Drop for RuntimeHandle {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

/// Open and migrate storage, initialize durable identity, and only then bind.
/// The returned handle makes startup and complete shutdown independently testable.
pub async fn start(
    config: ServerConfig,
    turso: TursoConfig,
) -> Result<RuntimeHandle, RuntimeError> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let sleeper: Arc<dyn Sleeper> = Arc::new(SystemClock);
    start_with_time(config, turso, clock, sleeper).await
}

/// Start a runtime with injectable time primitives. Exactly one scheduler actor is owned by the
/// returned runtime and is joined during shutdown before storage closes.
pub async fn start_with_time(
    config: ServerConfig,
    turso: TursoConfig,
    clock: Arc<dyn Clock>,
    sleeper: Arc<dyn Sleeper>,
) -> Result<RuntimeHandle, RuntimeError> {
    config.validate()?;
    let database = AttentionDatabase::open(turso).await?;
    database.run_startup_migrations().await?;
    let persistent = database
        .load_or_create_server_identity(PersistentServerIdentity::generate())
        .await?;
    let publications = PublicationHub::new(config.publication_capacity);
    let concrete = Arc::new(TursoAttentionService::new(
        database.clone(),
        publications.clone(),
    ));
    let read_concrete = Arc::clone(&concrete);
    let read_service: crate::SharedService = read_concrete;
    let mutation_concrete = Arc::clone(&concrete);
    let mutation_service: crate::SharedMutationService = mutation_concrete;
    let delivery_service: crate::SharedDeliveryWorkerService =
        Arc::new(crate::DeliveryPortService(database.clone()));
    let identity = ServerIdentity::new(
        protocol::ServerId(persistent.server_id.to_string()),
        protocol::StreamId(persistent.stream_id.to_string()),
    );
    let state = AppState::with_mutations(
        config.clone(),
        identity.clone(),
        read_service,
        mutation_service,
        delivery_service,
        publications,
    )?;
    let listener = TcpListener::bind(config.bind).await?;
    let address = listener.local_addr()?;
    let shutdown = state.shutdown.clone();
    let scheduler_shutdown = shutdown.clone();
    let scheduler_config = Arc::clone(&state.config);
    let scheduler_service = Arc::clone(&concrete);
    let scheduler = tokio::spawn(crate::scheduler::run(
        scheduler_service,
        scheduler_config,
        clock,
        sleeper,
        scheduler_shutdown,
    ));
    let task = tokio::spawn(async move {
        let result = serve(listener, Arc::clone(&state)).await;
        state.shutdown.cancel();
        let _ownership = state.wait_for_service_tasks().await;
        scheduler.await?;
        database.close().await?;
        result.map_err(RuntimeError::from)
    });
    Ok(RuntimeHandle {
        address,
        identity,
        shutdown,
        task,
    })
}

pub async fn run(config: ServerConfig, turso: TursoConfig) -> Result<(), RuntimeError> {
    let handle = start(config, turso).await?;
    tokio::signal::ctrl_c().await?;
    handle.shutdown().await
}
