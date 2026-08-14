use crate::config::ServerConfig;
use crate::publication::PublicationHub;
use crate::service::SharedDeliveryWorkerService;
use crate::service::SharedMutationService;
use crate::service::SharedService;
use attention_protocol as protocol;
use std::sync::Arc;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct ServerIdentity {
    pub server_id: protocol::ServerId,
    pub stream_id: protocol::StreamId,
    pub boot_id: protocol::BootId,
}
impl ServerIdentity {
    pub fn new(server_id: protocol::ServerId, stream_id: protocol::StreamId) -> Self {
        Self {
            server_id,
            stream_id,
            boot_id: protocol::BootId(uuid::Uuid::now_v7().to_string()),
        }
    }
}

pub struct AppState {
    pub config: Arc<ServerConfig>,
    pub identity: ServerIdentity,
    pub service: SharedService,
    pub mutations: Option<SharedMutationService>,
    pub delivery_workers: Option<SharedDeliveryWorkerService>,
    pub publications: PublicationHub,
    pub connection_slots: Arc<Semaphore>,
    pub shutdown: CancellationToken,
}
impl AppState {
    pub fn new(
        config: ServerConfig,
        identity: ServerIdentity,
        service: SharedService,
    ) -> Result<Arc<Self>, crate::ConfigError> {
        let publications = PublicationHub::new(config.publication_capacity);
        Self::build(config, identity, service, None, None, publications)
    }

    pub fn with_delivery_workers(
        config: ServerConfig,
        identity: ServerIdentity,
        service: SharedService,
        delivery_workers: SharedDeliveryWorkerService,
    ) -> Result<Arc<Self>, crate::ConfigError> {
        let publications = PublicationHub::new(config.publication_capacity);
        Self::build(
            config,
            identity,
            service,
            None,
            Some(delivery_workers),
            publications,
        )
    }

    pub fn with_mutations(
        config: ServerConfig,
        identity: ServerIdentity,
        service: SharedService,
        mutations: SharedMutationService,
        delivery_workers: SharedDeliveryWorkerService,
        publications: PublicationHub,
    ) -> Result<Arc<Self>, crate::ConfigError> {
        Self::build(
            config,
            identity,
            service,
            Some(mutations),
            Some(delivery_workers),
            publications,
        )
    }

    fn build(
        config: ServerConfig,
        identity: ServerIdentity,
        service: SharedService,
        mutations: Option<SharedMutationService>,
        delivery_workers: Option<SharedDeliveryWorkerService>,
        publications: PublicationHub,
    ) -> Result<Arc<Self>, crate::ConfigError> {
        config.validate()?;
        let max = config.max_connections;
        Ok(Arc::new(Self {
            config: Arc::new(config),
            identity,
            service,
            mutations,
            delivery_workers,
            publications,
            connection_slots: Arc::new(Semaphore::new(max)),
            shutdown: CancellationToken::new(),
        }))
    }

    /// Wait until every upgraded connection and all of its request tasks have
    /// exited. Holding the returned permits prevents new service work.
    pub async fn wait_for_service_tasks(&self) -> Option<OwnedSemaphorePermit> {
        let permits = u32::try_from(self.config.max_connections).ok()?;
        Arc::clone(&self.connection_slots)
            .acquire_many_owned(permits)
            .await
            .ok()
    }
}
