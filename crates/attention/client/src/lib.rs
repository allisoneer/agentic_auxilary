//! Bounded, reconnecting WebSocket SDK for Attention v1.

use attention_protocol as protocol;
use futures_util::SinkExt;
use futures_util::StreamExt;
use protocol::AttentionHelloResult;
use protocol::ChangeNotificationParams;
use protocol::HelloRequest;
use protocol::JsonRpcVersion;
use protocol::PROTOCOL_V1;
use protocol::RPC_HELLO_METHOD;
use protocol::RequestId;
use protocol::ResponseId;
use protocol::RpcMethod;
use protocol::RpcNotification;
use protocol::RpcNotificationMethod;
use protocol::RpcResponse;
use protocol::RpcResponsePayload;
use protocol::SubscriptionRequest;
use protocol::SubscriptionResult;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio::time::Instant;
use tokio::time::MissedTickBehavior;
use tokio_tungstenite::tungstenite::Message;

/// SDK configuration. Every queue and correlation map has an explicit bound.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub url: String,
    pub subscription: SubscriptionRequest,
    pub client_identity: Option<protocol::ClientIdentity>,
    pub command_capacity: usize,
    pub event_capacity: usize,
    pub max_pending: usize,
    pub request_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub heartbeat_timeout: Duration,
    pub reconnect_min: Duration,
    pub reconnect_max: Duration,
}

impl ClientConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            subscription: SubscriptionRequest::Snapshot,
            client_identity: None,
            command_capacity: 64,
            event_capacity: 256,
            max_pending: 32,
            request_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(15),
            heartbeat_timeout: Duration::from_secs(10),
            reconnect_min: Duration::from_millis(100),
            reconnect_max: Duration::from_secs(5),
        }
    }

    fn validate(&self) -> Result<(), ClientError> {
        if self.command_capacity == 0 || self.event_capacity == 0 || self.max_pending == 0 {
            return Err(ClientError::Configuration(
                "queue capacities must be non-zero",
            ));
        }
        if self.reconnect_min.is_zero() || self.reconnect_min > self.reconnect_max {
            return Err(ClientError::Configuration("invalid reconnect range"));
        }
        Ok(())
    }
}

/// Observable supervisor state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    Connected {
        server_id: protocol::ServerId,
        stream_id: protocol::StreamId,
    },
    Reconnecting {
        attempt: u32,
    },
    Gap,
    Closed,
}

/// Failures are classified by ownership and mutation ambiguity.
#[derive(Debug, Error, Clone)]
pub enum ClientError {
    #[error("peer RPC error: {0:?}")]
    Peer(protocol::RpcError),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("request timed out")]
    Timeout,
    #[error("local protocol/encoding error: {0}")]
    LocalProtocol(String),
    #[error("bounded client capacity exhausted: {0}")]
    Backpressure(&'static str),
    #[error("mutation outcome is ambiguous after transmission")]
    AmbiguousMutation,
    #[error("invalid cursor acknowledgement: {0}")]
    InvalidCursorAcknowledgement(&'static str),
    #[error("client is closed")]
    Closed,
    #[error("invalid configuration: {0}")]
    Configuration(&'static str),
}

/// A bounded record of an asynchronous connection failure.
#[derive(Debug, Clone)]
pub struct ClientIssue {
    pub error: ClientError,
}

/// A snapshot delivered by hello negotiation. Its cursor is not used for resume
/// until [`Client::acknowledge_snapshot`] is called after reducer application.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub state: protocol::AttentionSnapshot,
    pub after_cursor: protocol::Cursor,
}

/// Cloneable typed client handle.
#[derive(Clone)]
pub struct Client {
    tx: mpsc::Sender<Command>,
    status: watch::Receiver<ConnectionStatus>,
    ids: Arc<AtomicU64>,
    request_timeout: Duration,
}

/// Single-consumer subscription outputs.
pub struct Subscription {
    pub changes: mpsc::Receiver<protocol::ChangeEvent>,
    pub snapshots: mpsc::Receiver<Snapshot>,
    /// Asynchronous transport, negotiation, heartbeat, gap, and protocol failures.
    /// The bounded channel preserves the oldest unobserved records under pressure.
    pub issues: mpsc::Receiver<ClientIssue>,
}

struct Call {
    id: RequestId,
    method: &'static str,
    params: Value,
    mutation: bool,
    deadline: Instant,
    reply: oneshot::Sender<Result<Value, ClientError>>,
}

enum Command {
    Call(Call),
    Applied {
        cursor: protocol::Cursor,
        reply: oneshot::Sender<Result<(), ClientError>>,
    },
    SnapshotApplied {
        cursor: protocol::Cursor,
        reply: oneshot::Sender<Result<(), ClientError>>,
    },
    FreshSnapshot {
        reply: oneshot::Sender<Result<(), ClientError>>,
    },
    Close(oneshot::Sender<()>),
}

struct Pending {
    call: Call,
}

impl Client {
    /// Starts the supervisor. Initial connection happens asynchronously.
    pub fn connect(config: ClientConfig) -> Result<(Self, Subscription), ClientError> {
        config.validate()?;
        let (tx, rx) = mpsc::channel(config.command_capacity);
        let (event_tx, changes) = mpsc::channel(config.event_capacity);
        let (snapshot_tx, snapshots) = mpsc::channel(1);
        let (issue_tx, issues) = mpsc::channel(config.event_capacity);
        let (status_tx, status) = watch::channel(ConnectionStatus::Connecting);
        let request_timeout = config.request_timeout;
        tokio::spawn(run(config, rx, event_tx, snapshot_tx, issue_tx, status_tx));
        Ok((
            Self {
                tx,
                status,
                ids: Arc::new(AtomicU64::new(1)),
                request_timeout,
            },
            Subscription {
                changes,
                snapshots,
                issues,
            },
        ))
    }

    pub fn status(&self) -> watch::Receiver<ConnectionStatus> {
        self.status.clone()
    }

    /// Marks a change cursor as fully applied. Reconnect resumes strictly after it.
    pub async fn acknowledge_cursor(&self, cursor: protocol::Cursor) -> Result<(), ClientError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.tx
            .send(Command::Applied {
                cursor,
                reply: ack_tx,
            })
            .await
            .map_err(|_| ClientError::Closed)?;
        ack_rx.await.map_err(|_| ClientError::Closed)?
    }

    /// Marks a delivered snapshot as fully applied. Only then may reconnect resume
    /// after its cursor.
    pub async fn acknowledge_snapshot(
        &self,
        after_cursor: protocol::Cursor,
    ) -> Result<(), ClientError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.tx
            .send(Command::SnapshotApplied {
                cursor: after_cursor,
                reply: ack_tx,
            })
            .await
            .map_err(|_| ClientError::Closed)?;
        ack_rx.await.map_err(|_| ClientError::Closed)?
    }

    /// Drops the current resume frontier and reconnects this same client with a
    /// fresh snapshot subscription. This never creates a second supervisor.
    pub async fn request_fresh_snapshot(&self) -> Result<(), ClientError> {
        let (reply, response) = oneshot::channel();
        self.tx
            .send(Command::FreshSnapshot { reply })
            .await
            .map_err(|_| ClientError::Closed)?;
        response.await.map_err(|_| ClientError::Closed)?
    }

    /// Cleanly closes the WebSocket and supervisor.
    pub async fn close(&self) -> Result<(), ClientError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::Close(tx))
            .await
            .map_err(|_| ClientError::Closed)?;
        rx.await.map_err(|_| ClientError::Closed)
    }

    pub async fn call<M: RpcMethod>(&self, params: M::Params) -> Result<M::Result, ClientError> {
        self.call_kind::<M>(params, is_mutation_method(M::NAME))
            .await
    }

    async fn mutation<M: RpcMethod>(&self, params: M::Params) -> Result<M::Result, ClientError> {
        self.call_kind::<M>(params, true).await
    }

    async fn call_kind<M: RpcMethod>(
        &self,
        params: M::Params,
        mutation: bool,
    ) -> Result<M::Result, ClientError> {
        let id = RequestId(format!(
            "attention-client-{}",
            self.ids.fetch_add(1, Ordering::Relaxed)
        ));
        let params =
            serde_json::to_value(params).map_err(|e| ClientError::LocalProtocol(e.to_string()))?;
        let (tx, rx) = oneshot::channel();
        self.tx
            .try_send(Command::Call(Call {
                id,
                method: M::NAME,
                params,
                mutation,
                deadline: Instant::now() + self.request_timeout,
                reply: tx,
            }))
            .map_err(|e| match e {
                mpsc::error::TrySendError::Closed(_) => ClientError::Closed,
                mpsc::error::TrySendError::Full(_) => ClientError::Backpressure("writer queue"),
            })?;
        let value = rx.await.map_err(|_| ClientError::Closed)??;
        serde_json::from_value(value).map_err(|e| ClientError::LocalProtocol(e.to_string()))
    }
}

// Kept as an ordinary macro to make the complete protocol surface auditable.
macro_rules! methods {
    (queries { $($name:ident => $marker:path, $params:ty => $result:ty;)* }
     mutations { $($mname:ident => $mmarker:path, $mparams:ty => $mresult:ty;)* }) => {
        impl Client {
            $(pub async fn $name(&self, params: $params) -> Result<$result, ClientError> { self.call::<$marker>(params).await })*
            $(pub async fn $mname(&self, params: $mparams) -> Result<$mresult, ClientError> { self.mutation::<$mmarker>(params).await })*
        }
    };
}

methods! {
 queries {
  work_item_get => protocol::WorkItemGet, protocol::WorkItemGetParams => protocol::WorkItemView;
  attention_signal_get => protocol::AttentionSignalGet, protocol::AttentionSignalGetParams => protocol::AttentionSignalView;
  reminder_get => protocol::ReminderGet, protocol::ReminderGetParams => protocol::ReminderView;
  source_entity_get => protocol::SourceEntityGet, protocol::SourceEntityGetParams => protocol::SourceEntityView;
  source_receipt_get => protocol::SourceReceiptGet, protocol::SourceReceiptGetParams => protocol::SourceReceiptView;
  snapshot_get => protocol::SnapshotGet, protocol::EmptyParams => protocol::SnapshotResult;
  changes_get => protocol::ChangesGet, protocol::ChangesGetParams => protocol::ChangesResult;
  delivery_inspect => protocol::DeliveryInspect, protocol::DeliveryInspectParams => protocol::DeliveryInspectResult;
 }
 mutations {
  delivery_claim => protocol::DeliveryClaim, protocol::DeliveryClaimParams => protocol::DeliveryClaimResult;
  delivery_renew => protocol::DeliveryRenew, protocol::DeliveryRenewParams => protocol::DeliveryRenewResult;
  delivery_succeed => protocol::DeliverySucceed, protocol::DeliverySucceedParams => protocol::DeliverySucceedResult;
  delivery_fail_retryable => protocol::DeliveryFailRetryable, protocol::DeliveryFailRetryableParams => protocol::DeliveryFailRetryableResult;
  delivery_fail_terminal => protocol::DeliveryFailTerminal, protocol::DeliveryFailTerminalParams => protocol::DeliveryFailTerminalResult;
  work_item_create => protocol::WorkItemCreate, protocol::CreateWorkItemParams => protocol::CreateWorkItemResult;
  work_item_complete => protocol::WorkItemComplete, protocol::CompleteWorkItemParams => protocol::CompleteWorkItemResult;
  work_item_cancel => protocol::WorkItemCancel, protocol::CancelWorkItemParams => protocol::CancelWorkItemResult;
  attention_signal_acknowledge => protocol::AttentionSignalAcknowledge, protocol::AcknowledgeAttentionSignalParams => protocol::AcknowledgeAttentionSignalResult;
  source_occurrence_ingest => protocol::SourceOccurrenceIngest, protocol::IngestSourceOccurrenceParams => protocol::IngestSourceOccurrenceResult;
  reminder_create => protocol::ReminderCreate, protocol::CreateReminderParams => protocol::CreateReminderResult;
  reminder_fire_acknowledge => protocol::ReminderFireAcknowledge, protocol::AcknowledgeReminderFireParams => protocol::AcknowledgeReminderFireResult;
  reminder_fire_snooze => protocol::ReminderFireSnooze, protocol::SnoozeReminderFireParams => protocol::SnoozeReminderFireResult;
 }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorKind {
    Event,
    Snapshot,
}

#[derive(Debug, Clone)]
struct DeliveredCursor {
    cursor: protocol::Cursor,
    kind: CursorKind,
    sequence: u64,
}

/// Tracks cursors delivered to the consumer and the monotonic applied frontier.
/// The retained history is bounded so acknowledgement validation cannot grow
/// without limit when a consumer delays acknowledgements.
struct CursorTracker {
    capacity: usize,
    next_sequence: u64,
    delivered: VecDeque<DeliveredCursor>,
    acknowledged: Option<DeliveredCursor>,
}

impl CursorTracker {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            next_sequence: 0,
            delivered: VecDeque::new(),
            acknowledged: None,
        }
    }

    fn prune_acknowledged(&mut self) {
        let Some(sequence) = self.acknowledged.as_ref().map(|cursor| cursor.sequence) else {
            return;
        };
        while self
            .delivered
            .front()
            .is_some_and(|cursor| cursor.sequence <= sequence)
        {
            self.delivered.pop_front();
        }
    }

    fn record(&mut self, cursor: protocol::Cursor, kind: CursorKind) -> Result<u64, ClientError> {
        self.prune_acknowledged();
        if self.delivered.len() >= self.capacity {
            return Err(ClientError::Backpressure("cursor acknowledgement queue"));
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.delivered.push_back(DeliveredCursor {
            cursor,
            kind,
            sequence,
        });
        Ok(sequence)
    }

    fn rollback(&mut self, sequence: u64) {
        if self
            .delivered
            .back()
            .is_some_and(|cursor| cursor.sequence == sequence)
        {
            self.delivered.pop_back();
        }
    }

    fn reset(&mut self) {
        self.next_sequence = 0;
        self.delivered.clear();
        self.acknowledged = None;
    }

    fn acknowledge(
        &mut self,
        cursor: &protocol::Cursor,
        kind: CursorKind,
    ) -> Result<(), ClientError> {
        if self
            .acknowledged
            .as_ref()
            .is_some_and(|ack| ack.kind == kind && ack.cursor == *cursor)
        {
            return Ok(());
        }
        let Some(delivered) = self
            .delivered
            .iter()
            .rev()
            .find(|delivered| delivered.kind == kind && delivered.cursor == *cursor)
            .cloned()
        else {
            return Err(ClientError::InvalidCursorAcknowledgement(
                "cursor was not delivered by this client",
            ));
        };
        if self
            .acknowledged
            .as_ref()
            .is_some_and(|ack| delivered.sequence < ack.sequence)
        {
            return Err(ClientError::InvalidCursorAcknowledgement(
                "cursor acknowledgement regresses applied progress",
            ));
        }
        self.acknowledged = Some(delivered);
        self.prune_acknowledged();
        Ok(())
    }
}

fn apply_acknowledgement(
    cursors: &mut CursorTracker,
    kind: CursorKind,
    cursor: protocol::Cursor,
    identity: Option<&(protocol::ServerId, protocol::StreamId)>,
    resume: &mut Option<(protocol::ServerId, protocol::StreamId, protocol::Cursor)>,
) -> Result<(), ClientError> {
    let Some(identity) = identity else {
        return Err(ClientError::LocalProtocol(
            "cursor acknowledgement has no negotiated identity".into(),
        ));
    };
    cursors.acknowledge(&cursor, kind)?;
    *resume = Some((identity.0.clone(), identity.1.clone(), cursor));
    Ok(())
}

fn is_mutation_method(method: &str) -> bool {
    matches!(
        method,
        "attention.delivery.claim"
            | "attention.delivery.renew"
            | "attention.delivery.succeed"
            | "attention.delivery.fail_retryable"
            | "attention.delivery.fail_terminal"
            | "attention.work_item.create"
            | "attention.work_item.complete"
            | "attention.work_item.cancel"
            | "attention.signal.acknowledge"
            | "attention.source_occurrence.ingest"
            | "attention.reminder.create"
            | "attention.reminder_fire.acknowledge"
            | "attention.reminder_fire.snooze"
    )
}

async fn run(
    config: ClientConfig,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<protocol::ChangeEvent>,
    snapshots: mpsc::Sender<Snapshot>,
    issues: mpsc::Sender<ClientIssue>,
    status: watch::Sender<ConnectionStatus>,
) {
    let mut queued = VecDeque::new();
    let mut resume: Option<(protocol::ServerId, protocol::StreamId, protocol::Cursor)> = None;
    let mut last_identity: Option<(protocol::ServerId, protocol::StreamId)> = None;
    let mut cursors = CursorTracker::new(config.event_capacity.saturating_add(1));
    let mut attempt = 0u32;
    let mut force_snapshot = false;
    loop {
        expire_queued(&mut queued);
        let _ = status.send(if attempt == 0 {
            ConnectionStatus::Connecting
        } else {
            ConnectionStatus::Reconnecting { attempt }
        });
        let (mut ws, _) = match tokio_tungstenite::connect_async(&config.url).await {
            Ok(connection) => connection,
            Err(error) => {
                report(&issues, ClientError::Transport(error.to_string()));
                attempt = attempt.saturating_add(1);
                if wait_backoff(
                    &config,
                    attempt,
                    &mut commands,
                    &mut queued,
                    &status,
                    &mut cursors,
                    &mut resume,
                    last_identity.as_ref(),
                )
                .await
                {
                    return;
                }
                continue;
            }
        };
        let subscription = if force_snapshot {
            SubscriptionRequest::Snapshot
        } else {
            resume.as_ref().map_or_else(
                || config.subscription.clone(),
                |(server_id, stream_id, after_cursor)| SubscriptionRequest::Resume {
                    server_id: server_id.clone(),
                    stream_id: stream_id.clone(),
                    after_cursor: after_cursor.clone(),
                },
            )
        };
        let result = match hello(&mut ws, &config, subscription).await {
            Ok(result) => result,
            Err(ClientError::Peer(error)) if error.code == protocol::CURSOR_GAP => {
                report(&issues, ClientError::Peer(error));
                let _ = status.send(ConnectionStatus::Gap);
                cursors.reset();
                resume = None;
                force_snapshot = true;
                let _ = ws.close(None).await;
                continue;
            }
            Err(error) => {
                report(&issues, error);
                attempt = attempt.saturating_add(1);
                let _ = ws.close(None).await;
                continue;
            }
        };
        attempt = 0;
        let identity = (result.server_id.clone(), result.stream_id.clone());
        if last_identity
            .as_ref()
            .is_some_and(|previous| previous != &identity)
        {
            cursors.reset();
            resume = None;
        }
        last_identity = Some(identity.clone());
        let _ = status.send(ConnectionStatus::Connected {
            server_id: identity.0.clone(),
            stream_id: identity.1.clone(),
        });
        match result.subscription_result {
            SubscriptionResult::Snapshot {
                state,
                after_cursor,
            } => {
                force_snapshot = false;
                // A fresh snapshot supersedes every previously delivered cursor.
                cursors.reset();
                // Deliberately leave resume unset until the consumer acknowledges application.
                resume = None;
                let sequence = match cursors.record(after_cursor.clone(), CursorKind::Snapshot) {
                    Ok(sequence) => sequence,
                    Err(error) => {
                        report(&issues, error);
                        let _ = ws.close(None).await;
                        continue;
                    }
                };
                if snapshots
                    .try_send(Snapshot {
                        state,
                        after_cursor,
                    })
                    .is_err()
                {
                    cursors.rollback(sequence);
                    report(&issues, ClientError::Backpressure("snapshot queue"));
                    let _ = ws.close(None).await;
                    continue;
                }
            }
            SubscriptionResult::Resume { after_cursor } => {
                resume = Some((identity.0.clone(), identity.1.clone(), after_cursor));
            }
            SubscriptionResult::None => {}
        }
        match connected_loop(
            &config,
            &mut ws,
            &mut commands,
            &mut queued,
            &events,
            &issues,
            &identity,
            &mut resume,
            &mut cursors,
            &status,
        )
        .await
        {
            ConnectedExit::Closed => return,
            ConnectedExit::FreshSnapshot => force_snapshot = true,
            ConnectedExit::Reconnect => {}
        }
        attempt = 1;
    }
}

async fn hello(
    ws: &mut Ws,
    config: &ClientConfig,
    subscription: SubscriptionRequest,
) -> Result<AttentionHelloResult, ClientError> {
    let id = RequestId("attention-client-hello".to_string());
    let request = protocol::RpcRequest {
        jsonrpc: JsonRpcVersion,
        id: id.clone(),
        method: RPC_HELLO_METHOD.to_string(),
        params: Some(HelloRequest {
            protocol_version: PROTOCOL_V1,
            subscription,
            client: config.client_identity.clone(),
        }),
    };
    let text =
        serde_json::to_string(&request).map_err(|e| ClientError::LocalProtocol(e.to_string()))?;
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| ClientError::Transport(e.to_string()))?;
    let message = tokio::time::timeout(config.request_timeout, ws.next())
        .await
        .map_err(|_| ClientError::Timeout)?
        .ok_or_else(|| ClientError::Transport("closed during hello".into()))?
        .map_err(|e| ClientError::Transport(e.to_string()))?;
    let response: RpcResponse<Value> = serde_json::from_slice(&message.into_data())
        .map_err(|e| ClientError::LocalProtocol(e.to_string()))?;
    if response.id != ResponseId::Request(id) {
        return Err(ClientError::LocalProtocol(
            "hello response correlation mismatch".into(),
        ));
    }
    match response.payload {
        RpcResponsePayload::Success(value) => {
            let result: AttentionHelloResult = serde_json::from_value(value)
                .map_err(|e| ClientError::LocalProtocol(e.to_string()))?;
            if result.protocol_version != PROTOCOL_V1 {
                return Err(ClientError::LocalProtocol(
                    "peer selected an unsupported protocol version".into(),
                ));
            }
            Ok(result)
        }
        RpcResponsePayload::Error(error) => Err(ClientError::Peer(error)),
    }
}

enum ConnectedExit {
    Closed,
    Reconnect,
    FreshSnapshot,
}

#[expect(clippy::too_many_arguments, reason = "explicit supervisor resources")]
async fn connected_loop(
    config: &ClientConfig,
    ws: &mut Ws,
    commands: &mut mpsc::Receiver<Command>,
    queued: &mut VecDeque<Call>,
    events: &mpsc::Sender<protocol::ChangeEvent>,
    issues: &mpsc::Sender<ClientIssue>,
    identity: &(protocol::ServerId, protocol::StreamId),
    resume: &mut Option<(protocol::ServerId, protocol::StreamId, protocol::Cursor)>,
    cursors: &mut CursorTracker,
    status: &watch::Sender<ConnectionStatus>,
) -> ConnectedExit {
    let mut pending: HashMap<RequestId, Pending> = HashMap::new();
    let mut heartbeat = tokio::time::interval(config.heartbeat_interval);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // Consume interval's immediate first tick; the first ping is after a full interval.
    heartbeat.tick().await;
    let far_future = Instant::now() + Duration::from_hours(8760);
    let mut pong_deadline: Option<Instant> = None;
    loop {
        expire_queued(queued);
        while pending.len() < config.max_pending {
            let Some(call) = queued.pop_front() else {
                break;
            };
            if call.deadline <= Instant::now() {
                let _ = call.reply.send(Err(ClientError::Timeout));
                continue;
            }
            if send_call(ws, call, &mut pending).await.is_err() {
                report(issues, ClientError::Transport("request send failed".into()));
                disconnect_pending(pending, queued);
                return ConnectedExit::Reconnect;
            }
        }
        let request_deadline = pending
            .values()
            .map(|p| p.call.deadline)
            .min()
            .unwrap_or(far_future);
        let pong_at = pong_deadline.unwrap_or(far_future);
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Call(call)) => {
                    if pending.len() >= config.max_pending { enqueue_bounded(queued, call, config.command_capacity); }
                    else if send_call(ws, call, &mut pending).await.is_err() {
                        report(issues, ClientError::Transport("request send failed".into()));
                        disconnect_pending(pending, queued);
                        return ConnectedExit::Reconnect;
                    }
                }
                Some(Command::Applied { cursor, reply }) => {
                    let result = apply_acknowledgement(
                        cursors,
                        CursorKind::Event,
                        cursor,
                        Some(identity),
                        resume,
                    );
                    let _ = reply.send(result);
                }
                Some(Command::SnapshotApplied { cursor, reply }) => {
                    let result = apply_acknowledgement(
                        cursors,
                        CursorKind::Snapshot,
                        cursor,
                        Some(identity),
                        resume,
                    );
                    let _ = reply.send(result);
                }
                Some(Command::FreshSnapshot { reply }) => {
                    cursors.reset();
                    *resume = None;
                    let _ = reply.send(Ok(()));
                    let _ = ws.close(None).await;
                    disconnect_pending(pending, queued);
                    return ConnectedExit::FreshSnapshot;
                }
                Some(Command::Close(done)) => { fail_all(pending, queued); let _ = ws.close(None).await; let _ = status.send(ConnectionStatus::Closed); let _ = done.send(()); return ConnectedExit::Closed; }
                None => { let _ = ws.close(None).await; let _ = status.send(ConnectionStatus::Closed); return ConnectedExit::Closed; }
            },
            message = ws.next() => match message {
                Some(Ok(Message::Text(text))) => if let Err(error) = handle_message(text.as_bytes(), &mut pending, events, cursors) {
                    report(issues, error);
                    disconnect_pending(pending, queued);
                    return ConnectedExit::Reconnect;
                },
                Some(Ok(Message::Pong(_))) => pong_deadline = None,
                Some(Ok(Message::Ping(data))) => { if let Err(error) = ws.send(Message::Pong(data)).await {
                    report(issues, ClientError::Transport(error.to_string()));
                    disconnect_pending(pending, queued);
                    return ConnectedExit::Reconnect;
                } }
                Some(Ok(Message::Close(_))) | None => {
                    report(issues, ClientError::Transport("connection closed".into()));
                    disconnect_pending(pending, queued);
                    return ConnectedExit::Reconnect;
                }
                Some(Err(error)) => {
                    report(issues, ClientError::Transport(error.to_string()));
                    disconnect_pending(pending, queued);
                    return ConnectedExit::Reconnect;
                }
                Some(Ok(_)) => {}
            },
            () = tokio::time::sleep_until(pong_at), if pong_deadline.is_some() => {
                report(issues, ClientError::Transport("heartbeat pong timed out".into()));
                disconnect_pending(pending, queued);
                return ConnectedExit::Reconnect;
            }
            () = tokio::time::sleep_until(request_deadline), if !pending.is_empty() => {
                let now = Instant::now();
                let expired: Vec<_> = pending.iter().filter(|(_, p)| p.call.deadline <= now).map(|(id, _)| id.clone()).collect();
                for id in expired {
                    if let Some(p) = pending.remove(&id) {
                        let error = if p.call.mutation { ClientError::AmbiguousMutation } else { ClientError::Timeout };
                        let _ = p.call.reply.send(Err(error));
                    }
                }
            }
            _ = heartbeat.tick(), if pong_deadline.is_none() => {
                if let Err(error) = ws.send(Message::Ping(Vec::new().into())).await {
                    report(issues, ClientError::Transport(error.to_string()));
                    disconnect_pending(pending, queued);
                    return ConnectedExit::Reconnect;
                }
                pong_deadline = Some(Instant::now() + config.heartbeat_timeout);
            }
        }
    }
}

async fn send_call(
    ws: &mut Ws,
    call: Call,
    pending: &mut HashMap<RequestId, Pending>,
) -> Result<(), ()> {
    let request = protocol::RpcRequest {
        jsonrpc: JsonRpcVersion,
        id: call.id.clone(),
        method: call.method.to_string(),
        params: Some(call.params.clone()),
    };
    let Ok(text) = serde_json::to_string(&request) else {
        let _ = call.reply.send(Err(ClientError::LocalProtocol(
            "request encoding failed".into(),
        )));
        return Ok(());
    };
    // From this point transmission is attempted. Any mutation failure is ambiguous.
    if ws.send(Message::Text(text.into())).await.is_err() {
        if call.mutation {
            let _ = call.reply.send(Err(ClientError::AmbiguousMutation));
        } else {
            pending.insert(call.id.clone(), Pending { call });
        }
        return Err(());
    }
    pending.insert(call.id.clone(), Pending { call });
    Ok(())
}

fn handle_message(
    bytes: &[u8],
    pending: &mut HashMap<RequestId, Pending>,
    events: &mpsc::Sender<protocol::ChangeEvent>,
    cursors: &mut CursorTracker,
) -> Result<(), ClientError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| ClientError::LocalProtocol(e.to_string()))?;
    if value.get("method").is_some() {
        let notification: RpcNotification<ChangeNotificationParams> =
            serde_json::from_value(value).map_err(|e| ClientError::LocalProtocol(e.to_string()))?;
        if notification.method != protocol::AttentionChange::NAME {
            return Err(ClientError::LocalProtocol("unknown notification".into()));
        }
        let params = notification
            .params
            .ok_or_else(|| ClientError::LocalProtocol("notification params missing".into()))?;
        let event = params.event;
        let sequence = cursors.record(event.cursor.clone(), CursorKind::Event)?;
        if events.try_send(event).is_err() {
            cursors.rollback(sequence);
            return Err(ClientError::Backpressure("event queue"));
        }
        return Ok(());
    }
    let response: RpcResponse<Value> =
        serde_json::from_value(value).map_err(|e| ClientError::LocalProtocol(e.to_string()))?;
    let ResponseId::Request(id) = response.id else {
        return Err(ClientError::LocalProtocol("invalid response id".into()));
    };
    let Some(pending) = pending.remove(&id) else {
        return Err(ClientError::LocalProtocol("unknown response id".into()));
    };
    let result = match response.payload {
        RpcResponsePayload::Success(value) => Ok(value),
        RpcResponsePayload::Error(error) => Err(ClientError::Peer(error)),
    };
    let _ = pending.call.reply.send(result);
    Ok(())
}

fn report(issues: &mpsc::Sender<ClientIssue>, error: ClientError) {
    // Preserve already-buffered issues rather than evicting them.
    let _ = issues.try_send(ClientIssue { error });
}

fn expire_queued(queued: &mut VecDeque<Call>) {
    let now = Instant::now();
    let mut retained = VecDeque::with_capacity(queued.len());
    while let Some(call) = queued.pop_front() {
        if call.deadline <= now {
            let _ = call.reply.send(Err(ClientError::Timeout));
        } else {
            retained.push_back(call);
        }
    }
    *queued = retained;
}

fn disconnect_pending(pending: HashMap<RequestId, Pending>, queued: &mut VecDeque<Call>) {
    for (_, pending) in pending {
        if pending.call.mutation {
            let _ = pending.call.reply.send(Err(ClientError::AmbiguousMutation));
        } else {
            queued.push_back(pending.call);
        }
    }
}

fn fail_all(pending: HashMap<RequestId, Pending>, queued: &mut VecDeque<Call>) {
    for (_, p) in pending {
        let _ = p.call.reply.send(Err(ClientError::Closed));
    }
    while let Some(call) = queued.pop_front() {
        let _ = call.reply.send(Err(ClientError::Closed));
    }
}

fn enqueue_bounded(queued: &mut VecDeque<Call>, call: Call, capacity: usize) {
    if queued.len() >= capacity {
        let _ = call
            .reply
            .send(Err(ClientError::Backpressure("pending request queue")));
    } else {
        queued.push_back(call);
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit supervisor acknowledgement state"
)]
async fn wait_backoff(
    config: &ClientConfig,
    attempt: u32,
    commands: &mut mpsc::Receiver<Command>,
    queued: &mut VecDeque<Call>,
    status: &watch::Sender<ConnectionStatus>,
    cursors: &mut CursorTracker,
    resume: &mut Option<(protocol::ServerId, protocol::StreamId, protocol::Cursor)>,
    identity: Option<&(protocol::ServerId, protocol::StreamId)>,
) -> bool {
    let shift = attempt.saturating_sub(1).min(16);
    let delay = config
        .reconnect_min
        .saturating_mul(1u32 << shift)
        .min(config.reconnect_max);
    tokio::select! {
        () = tokio::time::sleep(delay) => false,
        command = commands.recv() => match command {
            Some(Command::Call(call)) => { enqueue_bounded(queued, call, config.command_capacity); false }
            Some(Command::Applied { cursor, reply }) => {
                let result = apply_acknowledgement(
                    cursors,
                    CursorKind::Event,
                    cursor,
                    identity,
                    resume,
                );
                let _ = reply.send(result);
                false
            }
            Some(Command::SnapshotApplied { cursor, reply }) => {
                let result = apply_acknowledgement(
                    cursors,
                    CursorKind::Snapshot,
                    cursor,
                    identity,
                    resume,
                );
                let _ = reply.send(result);
                false
            },
            Some(Command::FreshSnapshot { reply }) => {
                cursors.reset();
                *resume = None;
                let _ = reply.send(Ok(()));
                false
            }
            Some(Command::Close(done)) => { fail_all(HashMap::new(), queued); let _ = status.send(ConnectionStatus::Closed); let _ = done.send(()); true }
            None => { let _ = status.send(ConnectionStatus::Closed); true }
        }
    }
}
