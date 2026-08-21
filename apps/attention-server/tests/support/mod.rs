use attention_kernel as k;
use attention_server::AppState;
use attention_server::AttentionService;
use attention_server::ServerConfig;
use attention_server::ServerIdentity;
use attention_server::ServiceError;
use attention_server::SharedDeliveryWorkerService;
use attention_server::serve;
use chrono::DateTime;
use chrono::Utc;
use futures_util::future::BoxFuture;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use tokio::net::TcpListener;
use tokio::sync::Notify;

pub const SERVER_ID: &str = "server-test";
pub const STREAM_ID: &str = "stream-test";
pub const WAIT: std::time::Duration = std::time::Duration::from_secs(3);

pub fn cursor(value: u64) -> k::CommitCursor {
    k::CommitCursor::try_from(value).expect("positive test cursor")
}

pub fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("valid test timestamp")
        .with_timezone(&Utc)
}

pub fn event(value: u64) -> k::ChangeEvent {
    k::ChangeEventDraft::new(
        k::ChangeEventId::new(),
        at("2026-08-13T12:00:00Z"),
        k::ChangeKind::WorkItemCreated,
        vec![],
        k::InboxEffects::default(),
    )
    .commit(cursor(value))
}

pub struct Barrier {
    armed: AtomicBool,
    entered: Notify,
    release: Notify,
}
impl Default for Barrier {
    fn default() -> Self {
        Self {
            armed: AtomicBool::new(true),
            entered: Notify::new(),
            release: Notify::new(),
        }
    }
}
impl Barrier {
    pub async fn wait_entered(&self) {
        tokio::time::timeout(WAIT, self.entered.notified())
            .await
            .expect("barrier entry timeout");
    }
    pub fn enter(&self) -> bool {
        let entered = self.armed.swap(false, Ordering::AcqRel);
        if entered {
            self.entered.notify_one();
        }
        entered
    }
    pub async fn block(&self) {
        self.release.notified().await;
    }
    pub fn release(&self) {
        self.release.notify_waiters();
    }
}

#[derive(Clone)]
pub enum ChangeScript {
    Events(Arc<Mutex<Vec<k::ChangeEvent>>>),
    Gap(k::ChangeGap),
}

pub struct ScriptedService {
    pub work_item: Option<k::WorkItem>,
    pub signal: Option<k::AttentionSignal>,
    pub reminder: Option<k::Reminder>,
    pub entity: Option<k::SourceEntity>,
    pub receipt: Option<k::SourceReceipt>,
    pub snapshot: k::AttentionSnapshot,
    pub changes: ChangeScript,
    pub snapshot_barrier: Option<Arc<Barrier>>,
    pub changes_barrier: Option<Arc<Barrier>>,
    pub read_barrier: Option<Arc<Barrier>>,
}

impl ScriptedService {
    pub fn empty(snapshot_cursor: u64) -> Self {
        Self {
            work_item: None,
            signal: None,
            reminder: None,
            entity: None,
            receipt: None,
            snapshot: k::AttentionSnapshot::new(cursor(snapshot_cursor), vec![], vec![], vec![]),
            changes: ChangeScript::Events(Arc::new(Mutex::new(vec![]))),
            snapshot_barrier: None,
            changes_barrier: None,
            read_barrier: None,
        }
    }

    async fn maybe_block(barrier: Option<&Arc<Barrier>>) {
        if let Some(barrier) = barrier
            && barrier.enter()
        {
            barrier.block().await;
        }
    }
}

impl AttentionService for ScriptedService {
    fn work_item(
        &self,
        _: k::WorkItemId,
    ) -> BoxFuture<'_, Result<Option<k::WorkItem>, ServiceError>> {
        Box::pin(async move {
            Self::maybe_block(self.read_barrier.as_ref()).await;
            Ok(self.work_item.clone())
        })
    }
    fn attention_signal(
        &self,
        _: k::AttentionSignalId,
    ) -> BoxFuture<'_, Result<Option<k::AttentionSignal>, ServiceError>> {
        Box::pin(async move { Ok(self.signal.clone()) })
    }
    fn reminder(
        &self,
        _: k::ReminderId,
    ) -> BoxFuture<'_, Result<Option<k::Reminder>, ServiceError>> {
        Box::pin(async move { Ok(self.reminder.clone()) })
    }
    fn source_entity(
        &self,
        _: k::SourceAuthorityQuery,
    ) -> BoxFuture<'_, Result<Option<k::SourceEntity>, ServiceError>> {
        Box::pin(async move { Ok(self.entity.clone()) })
    }
    fn source_receipt(
        &self,
        _: k::SourceReceiptId,
    ) -> BoxFuture<'_, Result<Option<k::SourceReceipt>, ServiceError>> {
        Box::pin(async move { Ok(self.receipt.clone()) })
    }
    fn snapshot(&self) -> BoxFuture<'_, Result<k::AttentionSnapshot, ServiceError>> {
        Box::pin(async move {
            Self::maybe_block(self.snapshot_barrier.as_ref()).await;
            Ok(self.snapshot.clone())
        })
    }
    fn changes_after(
        &self,
        query: k::ChangesAfterQuery,
    ) -> BoxFuture<'_, Result<k::ChangesResult, ServiceError>> {
        Box::pin(async move {
            Self::maybe_block(self.changes_barrier.as_ref()).await;
            match &self.changes {
                ChangeScript::Gap(gap) => Ok(k::ChangesResult::Gap(*gap)),
                ChangeScript::Events(events) => {
                    let events = events
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let page: Vec<_> = events
                        .iter()
                        .filter(|event| event.cursor() > query.after())
                        .take(query.limit().value())
                        .cloned()
                        .collect();
                    let resume = page
                        .last()
                        .map_or_else(|| query.after(), k::ChangeEvent::cursor);
                    let has_more = events.iter().any(|event| event.cursor() > resume);
                    Ok(k::ChangesResult::Page(k::ChangePage::new(
                        page, resume, has_more,
                    )))
                }
            }
        })
    }
}

pub struct TestServer {
    pub state: Arc<AppState>,
    pub address: std::net::SocketAddr,
    task: tokio::task::JoinHandle<Result<(), attention_server::ServerError>>,
}
impl TestServer {
    pub async fn start(config: ServerConfig, service: Arc<dyn AttentionService>) -> Self {
        let state = AppState::new(
            config,
            ServerIdentity::new(
                attention_protocol::ServerId(SERVER_ID.into()),
                attention_protocol::StreamId(STREAM_ID.into()),
            ),
            service,
        )
        .expect("valid test state");
        Self::start_state(state).await
    }

    pub async fn start_with_delivery_workers(
        config: ServerConfig,
        service: Arc<dyn AttentionService>,
        delivery_workers: SharedDeliveryWorkerService,
    ) -> Self {
        let state = AppState::with_delivery_workers(
            config,
            ServerIdentity::new(
                attention_protocol::ServerId(SERVER_ID.into()),
                attention_protocol::StreamId(STREAM_ID.into()),
            ),
            service,
            delivery_workers,
        )
        .expect("valid test state");
        Self::start_state(state).await
    }

    async fn start_state(state: Arc<AppState>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let address = listener.local_addr().expect("test address");
        let task = tokio::spawn(serve(listener, Arc::clone(&state)));
        Self {
            state,
            address,
            task,
        }
    }
    pub fn url(&self) -> String {
        format!("ws://{}/v1/ws", self.address)
    }
    pub async fn shutdown(self) {
        self.state.shutdown.cancel();
        tokio::time::timeout(WAIT, self.task)
            .await
            .expect("server shutdown timeout")
            .expect("server join")
            .expect("server result");
    }
}
