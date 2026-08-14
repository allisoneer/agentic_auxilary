use crate::dto::ChangeEventDto;
use crate::dto::ConnectionStatusDto;
use crate::dto::DesktopMessageDto;
use crate::dto::DesktopStateDto;
use crate::dto::IssueDto;
use crate::dto::ResetReason;
use crate::dto::SnapshotDto;
use crate::error::DesktopErrorDto;
use crate::mutation::AcknowledgeFireInput;
use crate::mutation::AcknowledgeSignalInput;
use crate::mutation::CreateReminderInput;
use crate::mutation::CreateWorkItemInput;
use crate::mutation::ExistingWorkItemInput;
use crate::mutation::MutationReceiptDto;
use crate::mutation::SnoozeFireInput;
use crate::mutation::{self};
use attention_client::Client;
use attention_client::ClientConfig;
use attention_client::ClientError;
use attention_client::ConnectionStatus;
use attention_client::Subscription;
use attention_protocol::Cursor;
use attention_protocol::ServerId;
use attention_protocol::StreamId;
use attention_protocol::SubscriptionRequest;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use tauri::AppHandle;
use tauri::Emitter;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

const MAX_PENDING_CHANGES: usize = 256;
const MAX_REPLAY_MESSAGES: usize = 512;
const EVENT_NAME: &str = "attention://message";
type MessageEmitter = Arc<dyn Fn(DesktopMessageDto) -> Result<(), ()> + Send + Sync>;

struct Shared {
    sequence: u64,
    generation: u64,
    status: ConnectionStatus,
    identity: Option<(ServerId, StreamId)>,
    snapshot: Option<SnapshotDto>,
    issue: Option<IssueDto>,
    pending_snapshot: Option<String>,
    pending_changes: VecDeque<String>,
    replay: VecDeque<DesktopMessageDto>,
    gap_active: bool,
}

pub struct DesktopSupervisor {
    transport: Transport,
    shared: Arc<RwLock<Shared>>,
    acknowledgements: Mutex<()>,
    forwarding: Mutex<Option<JoinHandle<()>>>,
    closed: AtomicBool,
}

enum Transport {
    Live(Client),
    #[cfg(test)]
    Test(Arc<TestTransport>),
}

#[cfg(test)]
#[derive(Debug)]
pub enum TestMutationCall {
    CreateWorkItem(attention_protocol::CreateWorkItemParams),
    CompleteWorkItem(attention_protocol::CompleteWorkItemParams),
    CancelWorkItem(attention_protocol::CancelWorkItemParams),
    AcknowledgeSignal(attention_protocol::AcknowledgeAttentionSignalParams),
    CreateReminder(attention_protocol::CreateReminderParams),
    AcknowledgeFire(attention_protocol::AcknowledgeReminderFireParams),
    SnoozeFire(attention_protocol::SnoozeReminderFireParams),
}

#[cfg(test)]
#[derive(Debug)]
pub enum TestMutationResult {
    WorkItem(Result<attention_protocol::CreateWorkItemResult, ClientError>),
    Signal(Result<attention_protocol::AcknowledgeAttentionSignalResult, ClientError>),
    Reminder(Result<attention_protocol::CreateReminderResult, ClientError>),
}

#[cfg(test)]
#[derive(Default)]
pub struct TestTransport {
    pub acknowledgements: std::sync::Mutex<Vec<(bool, String)>>,
    pub acknowledgement_results: std::sync::Mutex<VecDeque<Result<(), ClientError>>>,
    pub mutation_calls: std::sync::Mutex<Vec<TestMutationCall>>,
    pub mutation_results: std::sync::Mutex<VecDeque<TestMutationResult>>,
    pub closed: AtomicBool,
}

#[cfg(test)]
impl TestTransport {
    pub fn script_acknowledgements(
        &self,
        results: impl IntoIterator<Item = Result<(), ClientError>>,
    ) {
        self.acknowledgement_results
            .lock()
            .expect("test acknowledgement result lock")
            .extend(results);
    }

    fn acknowledge(&self, snapshot: bool, cursor: String) -> Result<(), ClientError> {
        self.acknowledgements
            .lock()
            .expect("test acknowledgement lock")
            .push((snapshot, cursor));
        self.acknowledgement_results
            .lock()
            .expect("test acknowledgement result lock")
            .pop_front()
            .unwrap_or(Ok(()))
    }

    pub fn script(&self, results: impl IntoIterator<Item = TestMutationResult>) {
        self.mutation_results
            .lock()
            .expect("test mutation result lock")
            .extend(results);
    }

    fn work_item(
        &self,
        call: TestMutationCall,
    ) -> Result<attention_protocol::CreateWorkItemResult, ClientError> {
        self.mutation_calls
            .lock()
            .expect("test mutation call lock")
            .push(call);
        let result = self
            .mutation_results
            .lock()
            .expect("test mutation result lock")
            .pop_front();
        match result {
            Some(TestMutationResult::WorkItem(result)) => result,
            _ => panic!("missing or mismatched scripted work-item mutation result"),
        }
    }

    fn signal(
        &self,
        call: TestMutationCall,
    ) -> Result<attention_protocol::AcknowledgeAttentionSignalResult, ClientError> {
        self.mutation_calls
            .lock()
            .expect("test mutation call lock")
            .push(call);
        let result = self
            .mutation_results
            .lock()
            .expect("test mutation result lock")
            .pop_front();
        match result {
            Some(TestMutationResult::Signal(result)) => result,
            _ => panic!("missing or mismatched scripted signal mutation result"),
        }
    }

    fn reminder(
        &self,
        call: TestMutationCall,
    ) -> Result<attention_protocol::CreateReminderResult, ClientError> {
        self.mutation_calls
            .lock()
            .expect("test mutation call lock")
            .push(call);
        let result = self
            .mutation_results
            .lock()
            .expect("test mutation result lock")
            .pop_front();
        match result {
            Some(TestMutationResult::Reminder(result)) => result,
            _ => panic!("missing or mismatched scripted reminder mutation result"),
        }
    }
}

impl DesktopSupervisor {
    pub fn start<R: tauri::Runtime>(
        app: AppHandle<R>,
        url: String,
    ) -> Result<Self, DesktopErrorDto> {
        let mut config = ClientConfig::new(url);
        config.subscription = SubscriptionRequest::Snapshot;
        let emitter: MessageEmitter =
            Arc::new(move |message| app.emit(EVENT_NAME, message).map_err(|_| ()));
        Self::start_with_config(config, emitter)
    }

    fn start_with_config(
        config: ClientConfig,
        emitter: MessageEmitter,
    ) -> Result<Self, DesktopErrorDto> {
        let (client, subscription) = Client::connect(config).map_err(DesktopErrorDto::from)?;
        let shared = Arc::new(RwLock::new(Shared {
            sequence: 0,
            generation: 1,
            status: ConnectionStatus::Connecting,
            identity: None,
            snapshot: None,
            issue: None,
            pending_snapshot: None,
            pending_changes: VecDeque::with_capacity(MAX_PENDING_CHANGES),
            replay: VecDeque::with_capacity(MAX_REPLAY_MESSAGES),
            gap_active: false,
        }));
        let forwarding = tokio::spawn(forward(
            emitter,
            Arc::clone(&shared),
            client.clone(),
            client.status(),
            subscription,
        ));
        Ok(Self {
            transport: Transport::Live(client),
            shared,
            acknowledgements: Mutex::new(()),
            forwarding: Mutex::new(Some(forwarding)),
            closed: AtomicBool::new(false),
        })
    }

    #[cfg(test)]
    pub(crate) fn live_for_test(
        config: ClientConfig,
        expected_identity: Option<(ServerId, StreamId)>,
    ) -> Result<Self, DesktopErrorDto> {
        let supervisor = Self::start_with_config(config, Arc::new(|_| Ok(())))?;
        if let Some(identity) = expected_identity {
            supervisor
                .shared
                .try_write()
                .expect("new supervisor state is uncontended")
                .identity = Some(identity);
        }
        Ok(supervisor)
    }

    #[cfg(test)]
    pub(crate) async fn expect_identity_for_test(&self, server_id: ServerId, stream_id: StreamId) {
        self.shared.write().await.identity = Some((server_id, stream_id));
    }

    /// Returns an atomic bootstrap: the materialized state and bounded ordered
    /// replay were captured under the same read lock.
    pub async fn state(&self) -> DesktopStateDto {
        let state = self.shared.read().await;
        DesktopStateDto {
            sequence: state.sequence,
            generation: state.generation,
            status: (&state.status).into(),
            snapshot: state.snapshot.clone(),
            snapshot_after_cursor: state.pending_snapshot.clone(),
            issue: state.issue.clone(),
            replay: state.replay.iter().cloned().collect(),
        }
    }

    pub async fn acknowledge_snapshot(
        &self,
        generation: u64,
        cursor: String,
    ) -> Result<(), DesktopErrorDto> {
        let _serial = self.acknowledgements.lock().await;
        {
            let state = self.shared.read().await;
            if state.generation != generation || state.pending_snapshot.as_deref() != Some(&cursor)
            {
                return Err(DesktopErrorDto::invalid_ack());
            }
        }
        match &self.transport {
            Transport::Live(client) => client
                .acknowledge_snapshot(Cursor(cursor.clone()))
                .await
                .map_err(DesktopErrorDto::from)?,
            #[cfg(test)]
            Transport::Test(transport) => transport
                .acknowledge(true, cursor.clone())
                .map_err(DesktopErrorDto::from)?,
        }
        let mut state = self.shared.write().await;
        if state.generation != generation || state.pending_snapshot.as_deref() != Some(&cursor) {
            return Err(DesktopErrorDto::invalid_ack());
        }
        let applied_sequence = state.replay.iter().find_map(|message| match message {
            DesktopMessageDto::Snapshot {
                sequence,
                after_cursor,
                ..
            } if after_cursor == &cursor => Some(*sequence),
            _ => None,
        });
        state.pending_snapshot = None;
        if let Some(sequence) = applied_sequence {
            compact_replay(&mut state, sequence);
        }
        Ok(())
    }

    pub async fn acknowledge_change(
        &self,
        generation: u64,
        cursor: String,
    ) -> Result<(), DesktopErrorDto> {
        let _serial = self.acknowledgements.lock().await;
        let mut state = self.shared.write().await;
        if state.generation != generation
            || state.pending_snapshot.is_some()
            || state.pending_changes.front() != Some(&cursor)
        {
            return Err(DesktopErrorDto::invalid_ack());
        }
        let applied = state.replay.iter().find_map(|message| match message {
            DesktopMessageDto::Change {
                sequence, event, ..
            } if event.cursor == cursor => Some((*sequence, event.clone())),
            _ => None,
        });
        state.pending_changes.pop_front();
        if let Some((sequence, event)) = applied {
            if let Some(snapshot) = &mut state.snapshot {
                snapshot.apply(&event);
            }
            compact_replay(&mut state, sequence);
        }
        drop(state);
        match &self.transport {
            Transport::Live(client) => client
                .acknowledge_cursor(Cursor(cursor.clone()))
                .await
                .map_err(DesktopErrorDto::from)?,
            #[cfg(test)]
            Transport::Test(transport) => transport
                .acknowledge(false, cursor)
                .map_err(DesktopErrorDto::from)?,
        }
        Ok(())
    }

    pub async fn create_work_item(
        &self,
        input: CreateWorkItemInput,
    ) -> Result<MutationReceiptDto, DesktopErrorDto> {
        let params = mutation::create_work_item_params(&input)?;
        match &self.transport {
            Transport::Live(client) => client.work_item_create(params).await,
            #[cfg(test)]
            Transport::Test(transport) => {
                transport.work_item(TestMutationCall::CreateWorkItem(params))
            }
        }
        .map(mutation::work_item_receipt)
        .map_err(Into::into)
    }

    pub async fn complete_work_item(
        &self,
        input: ExistingWorkItemInput,
    ) -> Result<MutationReceiptDto, DesktopErrorDto> {
        let params = mutation::complete_work_item_params(input)?;
        match &self.transport {
            Transport::Live(client) => client.work_item_complete(params).await,
            #[cfg(test)]
            Transport::Test(transport) => {
                transport.work_item(TestMutationCall::CompleteWorkItem(params))
            }
        }
        .map(mutation::work_item_receipt)
        .map_err(Into::into)
    }

    pub async fn cancel_work_item(
        &self,
        input: ExistingWorkItemInput,
    ) -> Result<MutationReceiptDto, DesktopErrorDto> {
        let params = mutation::cancel_work_item_params(input)?;
        match &self.transport {
            Transport::Live(client) => client.work_item_cancel(params).await,
            #[cfg(test)]
            Transport::Test(transport) => {
                transport.work_item(TestMutationCall::CancelWorkItem(params))
            }
        }
        .map(mutation::work_item_receipt)
        .map_err(Into::into)
    }

    pub async fn acknowledge_signal(
        &self,
        input: AcknowledgeSignalInput,
    ) -> Result<MutationReceiptDto, DesktopErrorDto> {
        let params = mutation::acknowledge_signal_params(input)?;
        match &self.transport {
            Transport::Live(client) => client.attention_signal_acknowledge(params).await,
            #[cfg(test)]
            Transport::Test(transport) => {
                transport.signal(TestMutationCall::AcknowledgeSignal(params))
            }
        }
        .map(mutation::signal_receipt)
        .map_err(Into::into)
    }

    pub async fn create_reminder(
        &self,
        input: CreateReminderInput,
    ) -> Result<MutationReceiptDto, DesktopErrorDto> {
        let params = mutation::create_reminder_params(input)?;
        match &self.transport {
            Transport::Live(client) => client.reminder_create(params).await,
            #[cfg(test)]
            Transport::Test(transport) => {
                transport.reminder(TestMutationCall::CreateReminder(params))
            }
        }
        .map(mutation::reminder_receipt)
        .map_err(Into::into)
    }

    pub async fn acknowledge_fire(
        &self,
        input: AcknowledgeFireInput,
    ) -> Result<MutationReceiptDto, DesktopErrorDto> {
        let params = mutation::acknowledge_fire_params(input)?;
        match &self.transport {
            Transport::Live(client) => client.reminder_fire_acknowledge(params).await,
            #[cfg(test)]
            Transport::Test(transport) => {
                transport.reminder(TestMutationCall::AcknowledgeFire(params))
            }
        }
        .map(mutation::reminder_receipt)
        .map_err(Into::into)
    }

    pub async fn snooze_fire(
        &self,
        input: SnoozeFireInput,
    ) -> Result<MutationReceiptDto, DesktopErrorDto> {
        let params = mutation::snooze_fire_params(input)?;
        match &self.transport {
            Transport::Live(client) => client.reminder_fire_snooze(params).await,
            #[cfg(test)]
            Transport::Test(transport) => transport.reminder(TestMutationCall::SnoozeFire(params)),
        }
        .map(mutation::reminder_receipt)
        .map_err(Into::into)
    }

    pub async fn close(&self) -> Result<(), DesktopErrorDto> {
        if self.closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        match &self.transport {
            Transport::Live(client) => client.close().await.map_err(DesktopErrorDto::from)?,
            #[cfg(test)]
            Transport::Test(transport) => transport.closed.store(true, Ordering::Release),
        }
        let task = self.forwarding.lock().await.take();
        if let Some(task) = task {
            let _ = task.await;
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn for_test(
        generation: u64,
        pending_snapshot: Option<&str>,
        pending_changes: &[&str],
        replay: VecDeque<DesktopMessageDto>,
    ) -> (Self, Arc<TestTransport>) {
        let transport = Arc::new(TestTransport::default());
        let sequence = replay
            .back()
            .map(|message| match message {
                DesktopMessageDto::Status { sequence, .. }
                | DesktopMessageDto::Reset { sequence, .. }
                | DesktopMessageDto::Snapshot { sequence, .. }
                | DesktopMessageDto::Change { sequence, .. }
                | DesktopMessageDto::Issue { sequence, .. } => *sequence,
            })
            .unwrap_or_default();
        let shared = Shared {
            sequence,
            generation,
            status: ConnectionStatus::Connecting,
            identity: None,
            snapshot: None,
            issue: None,
            pending_snapshot: pending_snapshot.map(str::to_owned),
            pending_changes: pending_changes
                .iter()
                .map(|cursor| (*cursor).to_owned())
                .collect(),
            replay,
            gap_active: false,
        };
        (
            Self {
                transport: Transport::Test(Arc::clone(&transport)),
                shared: Arc::new(RwLock::new(shared)),
                acknowledgements: Mutex::new(()),
                forwarding: Mutex::new(None),
                closed: AtomicBool::new(false),
            },
            transport,
        )
    }

    #[cfg(test)]
    pub(crate) async fn force_reset(&self, reason: ResetReason) {
        let mut state = self.shared.write().await;
        reset(&mut state, reason);
    }

    #[cfg(test)]
    pub(crate) async fn fill_replay_to_overflow(&self) {
        let mut state = self.shared.write().await;
        while state.replay.len() < MAX_REPLAY_MESSAGES {
            state.sequence = state.sequence.saturating_add(1);
            let sequence = state.sequence;
            let generation = state.generation;
            state.replay.push_back(DesktopMessageDto::Status {
                sequence,
                generation,
                status: ConnectionStatusDto::Connecting,
            });
        }
        next_message(&mut state, |sequence, generation| {
            DesktopMessageDto::Status {
                sequence,
                generation,
                status: ConnectionStatusDto::Connecting,
            }
        });
    }

    #[cfg(test)]
    pub(crate) async fn set_snapshot_for_test(&self, snapshot: SnapshotDto) {
        self.shared.write().await.snapshot = Some(snapshot);
    }

    #[cfg(test)]
    pub(crate) async fn redeliver_change_for_test(&self, event: ChangeEventDto) {
        let mut state = self.shared.write().await;
        state.pending_changes.push_back(event.cursor.clone());
        next_message(&mut state, |sequence, generation| {
            DesktopMessageDto::Change {
                sequence,
                generation,
                event,
            }
        });
    }

    #[cfg(test)]
    pub(crate) async fn update_status_for_test(
        &self,
        current: ConnectionStatus,
    ) -> DesktopMessageDto {
        update_status(&self.shared, current).await
    }
}

async fn forward(
    emitter: MessageEmitter,
    shared: Arc<RwLock<Shared>>,
    client: Client,
    mut status: tokio::sync::watch::Receiver<ConnectionStatus>,
    mut subscription: Subscription,
) {
    loop {
        tokio::select! {
            biased;
            snapshot = subscription.snapshots.recv() => {
                let Some(snapshot) = snapshot else { break; };
                let message = {
                    let mut state = shared.write().await;
                    state.gap_active = false;
                    state.pending_changes.clear();
                    let dto = SnapshotDto::from(snapshot.state);
                    state.snapshot = Some(dto.clone());
                    state.pending_snapshot = Some(snapshot.after_cursor.0.clone());
                    next_message(&mut state, |sequence, generation| DesktopMessageDto::Snapshot {
                        sequence, generation, state: dto, after_cursor: snapshot.after_cursor.0
                    })
                };
                emit_or_reset(&emitter, &shared, &client, message).await;
            }
            changed = status.changed() => {
                if changed.is_err() { break; }
                let current = status.borrow_and_update().clone();
                let message = update_status(&shared, current).await;
                emit_or_reset(&emitter, &shared, &client, message).await;
            }
            change = subscription.changes.recv() => {
                let Some(change) = change else { break; };
                let message = {
                    let mut state = shared.write().await;
                    if state.snapshot.is_none() || state.pending_changes.len() >= MAX_PENDING_CHANGES {
                        reset(&mut state, ResetReason::Overflow)
                    } else {
                        state.pending_changes.push_back(change.cursor.0.clone());
                        next_message(&mut state, |sequence, generation| DesktopMessageDto::Change {
                            sequence, generation, event: ChangeEventDto::from(change)
                        })
                    }
                };
                emit_or_reset(&emitter, &shared, &client, message).await;
            }
            issue = subscription.issues.recv() => {
                let Some(issue) = issue else { break; };
                let message = {
                    let mut state = shared.write().await;
                    if matches!(&issue.error, ClientError::Peer(error) if error.code == attention_protocol::CURSOR_GAP) {
                        reset(&mut state, ResetReason::Gap)
                    } else {
                        let dto = sanitize_issue(&issue.error);
                        state.issue = Some(dto.clone());
                        next_message(&mut state, |sequence, generation| DesktopMessageDto::Issue {
                            sequence, generation, issue: dto
                        })
                    }
                };
                emit_or_reset(&emitter, &shared, &client, message).await;
            }
        }
    }
}

fn next_message(
    state: &mut Shared,
    make: impl FnOnce(u64, u64) -> DesktopMessageDto,
) -> DesktopMessageDto {
    state.sequence = state.sequence.saturating_add(1);
    if state.replay.len() == MAX_REPLAY_MESSAGES {
        state.generation = state.generation.saturating_add(1);
        state.snapshot = None;
        state.issue = None;
        state.pending_snapshot = None;
        state.pending_changes.clear();
        state.gap_active = true;
        state.replay.clear();
        let message = DesktopMessageDto::Reset {
            sequence: state.sequence,
            generation: state.generation,
            reason: ResetReason::Overflow,
        };
        state.replay.push_back(message.clone());
        return message;
    }
    let message = make(state.sequence, state.generation);
    match message {
        DesktopMessageDto::Status { .. } => state
            .replay
            .retain(|message| !matches!(message, DesktopMessageDto::Status { .. })),
        DesktopMessageDto::Issue { .. } => state
            .replay
            .retain(|message| !matches!(message, DesktopMessageDto::Issue { .. })),
        _ => {}
    }
    state.replay.push_back(message.clone());
    message
}

fn reset(state: &mut Shared, reason: ResetReason) -> DesktopMessageDto {
    state.generation = state.generation.saturating_add(1);
    state.snapshot = None;
    state.issue = None;
    state.pending_snapshot = None;
    state.pending_changes.clear();
    state.gap_active = true;
    state.replay.clear();
    next_message(state, |sequence, generation| DesktopMessageDto::Reset {
        sequence,
        generation,
        reason,
    })
}

async fn emit_or_reset(
    emitter: &MessageEmitter,
    shared: &RwLock<Shared>,
    client: &Client,
    message: DesktopMessageDto,
) {
    let needs_snapshot = matches!(
        message,
        DesktopMessageDto::Reset {
            reason: ResetReason::Overflow,
            ..
        }
    );
    if emitter(message).is_err() {
        let reset_message = {
            let mut state = shared.write().await;
            reset(&mut state, ResetReason::EmissionFailed)
        };
        let _ = emitter(reset_message);
        let _ = client.request_fresh_snapshot().await;
    } else if needs_snapshot {
        let _ = client.request_fresh_snapshot().await;
    }
}

async fn update_status(shared: &RwLock<Shared>, current: ConnectionStatus) -> DesktopMessageDto {
    let mut state = shared.write().await;
    let reset_reason = match &current {
        ConnectionStatus::Gap if !state.gap_active => Some(ResetReason::Gap),
        ConnectionStatus::Connected {
            server_id,
            stream_id,
        } if state
            .identity
            .as_ref()
            .is_some_and(|identity| identity != &(server_id.clone(), stream_id.clone())) =>
        {
            Some(ResetReason::StreamChanged)
        }
        _ => None,
    };
    if let ConnectionStatus::Connected {
        server_id,
        stream_id,
    } = &current
    {
        state.identity = Some((server_id.clone(), stream_id.clone()));
    }
    state.status = current.clone();
    if let Some(reason) = reset_reason {
        if matches!(reason, ResetReason::StreamChanged) {
            let fresh = state.snapshot.clone().zip(state.pending_snapshot.clone());
            let reset_message = reset(&mut state, reason);
            if let Some((snapshot, after_cursor)) = fresh {
                state.snapshot = Some(snapshot.clone());
                state.pending_snapshot = Some(after_cursor.clone());
                state.gap_active = false;
                return next_message(&mut state, |sequence, generation| {
                    DesktopMessageDto::Snapshot {
                        sequence,
                        generation,
                        state: snapshot,
                        after_cursor,
                    }
                });
            }
            reset_message
        } else {
            reset(&mut state, reason)
        }
    } else {
        next_message(&mut state, |sequence, generation| {
            DesktopMessageDto::Status {
                sequence,
                generation,
                status: ConnectionStatusDto::from(&current),
            }
        })
    }
}

fn compact_replay(state: &mut Shared, through: u64) {
    state.replay.retain(|message| match message {
        DesktopMessageDto::Snapshot { sequence, .. }
        | DesktopMessageDto::Change { sequence, .. } => *sequence > through,
        DesktopMessageDto::Status { .. }
        | DesktopMessageDto::Issue { .. }
        | DesktopMessageDto::Reset { .. } => true,
    });
}

fn sanitize_issue(error: &ClientError) -> IssueDto {
    let mapped = DesktopErrorDto::from(error.clone());
    IssueDto {
        category: mapped.category,
        message: mapped.message,
    }
}
