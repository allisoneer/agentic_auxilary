use crate::Error;
use crate::mapping;
use attention_kernel::AffectedView;
use attention_kernel::AttentionSignal;
use attention_kernel::AttentionSignalValue;
use attention_kernel::ChangeEvent;
use attention_kernel::ChangeEventDraft;
use attention_kernel::ChangeEventId;
use attention_kernel::ChangeKind;
use attention_kernel::CommandDisposition;
use attention_kernel::CommandOutcome;
use attention_kernel::CommitCursor;
use attention_kernel::ExternalEntityId;
use attention_kernel::InboxEffects;
use attention_kernel::InboxEntry;
use attention_kernel::MutationOperation;
use attention_kernel::NormalizedSourceOrder;
use attention_kernel::OccurrenceId;
use attention_kernel::OccurrenceKey;
use attention_kernel::PriorMutationOutcome;
use attention_kernel::ReceiptOnlyReason;
use attention_kernel::Reminder;
use attention_kernel::ReminderFire;
use attention_kernel::ReminderFireState;
use attention_kernel::ReminderTarget;
use attention_kernel::ReminderValue;
use attention_kernel::Revision;
use attention_kernel::SignalAttentionState;
use attention_kernel::SignalSourceLifecycle;
use attention_kernel::SourceComparatorDomain;
use attention_kernel::SourceEntity;
use attention_kernel::SourceEntityKey;
use attention_kernel::SourceIngestionDecision;
use attention_kernel::SourceIngestionValue;
use attention_kernel::SourceInstance;
use attention_kernel::SourceKind;
use attention_kernel::SourceOrderMode;
use attention_kernel::SourceReceipt;
use attention_kernel::SourceStateVersion;
use attention_kernel::WorkItem;
use attention_kernel::WorkItemLifecycle;
use attention_kernel::WorkItemValue;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;

pub const VERSION: i64 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct EventPayloadV1 {
    affected_views: Vec<ViewV1>,
    inbox_additions: Vec<InboxEntryV1>,
    inbox_removals: Vec<InboxEntryV1>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ViewV1 {
    WorkItem { value: WorkItemV1 },
    AttentionSignal { value: SignalV1 },
    Reminder { value: ReminderV1 },
    SourceReceipt { value: ReceiptV1 },
    SourceEntity { value: EntityV1 },
}

#[derive(Debug, Serialize, Deserialize)]
struct SourceKeyV1 {
    source_kind: String,
    source_instance: String,
    external_entity_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum OrderV1 {
    Unordered,
    Ordered {
        domain: String,
        value: Option<Vec<u8>>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkItemV1 {
    id: String,
    revision: u64,
    lifecycle: u8,
    due_at: Option<String>,
    scheduled_at: Option<String>,
    defer_until: Option<String>,
    source_link: Option<SourceKeyV1>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SignalV1 {
    id: String,
    revision: u64,
    source_lifecycle: u8,
    attention_state: u8,
    source_receipt_id: String,
    source_entity_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FireV1 {
    id: String,
    trigger_at: String,
    state: u8,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReminderV1 {
    id: String,
    revision: u64,
    target_kind: u8,
    target_id: String,
    trigger_at: String,
    fires: Vec<FireV1>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReceiptV1 {
    id: String,
    source_kind: String,
    source_instance: String,
    occurrence_id: String,
    source_entity_key: Option<SourceKeyV1>,
    fingerprint: Vec<u8>,
    source_order: OrderV1,
    occurred_at: String,
    ingested_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntityV1 {
    id: String,
    key: SourceKeyV1,
    version: u64,
    latest_receipt_id: String,
    order: OrderV1,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
enum InboxEntryV1 {
    WorkItem(String),
    AttentionSignal(String),
    ReminderFire(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct OutcomeV1 {
    operation: u8,
    cursor: u64,
    change_event_id: String,
    outbox_intent_id: Option<String>,
    value: OutcomeValueV1,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutcomeValueV1 {
    WorkItem {
        id: String,
    },
    AttentionSignal {
        id: String,
    },
    SourceIngestion {
        receipt_id: String,
        signal_id: String,
        decision: u8,
    },
    Reminder {
        reminder_id: String,
        fire_id: String,
    },
}

fn malformed(record: &'static str, version: i64, byte_length: usize) -> Error {
    Error::MalformedCodec {
        record,
        version,
        byte_length,
    }
}

fn decode_json<T: for<'de> Deserialize<'de>>(
    record: &'static str,
    version: i64,
    bytes: &[u8],
) -> Result<T, Error> {
    if version != VERSION {
        return Err(Error::UnsupportedCodec {
            record,
            version,
            byte_length: bytes.len(),
        });
    }
    serde_json::from_slice(bytes).map_err(|_| malformed(record, version, bytes.len()))
}

pub fn encode_event(draft: &ChangeEventDraft) -> Result<(i64, Vec<u8>), Error> {
    let payload = EventPayloadV1 {
        affected_views: draft.affected_views().iter().map(ViewV1::from).collect(),
        inbox_additions: draft
            .inbox_effects()
            .additions()
            .iter()
            .copied()
            .map(InboxEntryV1::from)
            .collect(),
        inbox_removals: draft
            .inbox_effects()
            .removals()
            .iter()
            .copied()
            .map(InboxEntryV1::from)
            .collect(),
    };
    serde_json::to_vec(&payload)
        .map(|mut bytes| {
            bytes.push(b'\n');
            (VERSION, bytes)
        })
        .map_err(|error| Error::Decode(Box::new(error)))
}

pub fn decode_event(
    cursor: CommitCursor,
    id: ChangeEventId,
    occurred_at: DateTime<Utc>,
    kind: ChangeKind,
    version: i64,
    bytes: &[u8],
) -> Result<ChangeEvent, Error> {
    let payload: EventPayloadV1 = decode_json("change event", version, bytes)?;
    let views = payload
        .affected_views
        .into_iter()
        .map(AffectedView::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| malformed("change event", version, bytes.len()))?;
    let additions = payload
        .inbox_additions
        .into_iter()
        .map(InboxEntry::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| malformed("change event", version, bytes.len()))?;
    let removals = payload
        .inbox_removals
        .into_iter()
        .map(InboxEntry::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| malformed("change event", version, bytes.len()))?;
    Ok(ChangeEventDraft::new(
        id,
        occurred_at,
        kind,
        views,
        InboxEffects::new(additions, removals),
    )
    .commit(cursor))
}

pub fn encode_outcome(outcome: &PriorMutationOutcome) -> Result<(i64, Vec<u8>), Error> {
    let value = OutcomeV1::from(outcome);
    serde_json::to_vec(&value)
        .map(|bytes| (VERSION, bytes))
        .map_err(|error| Error::Decode(Box::new(error)))
}

pub fn decode_outcome(version: i64, bytes: &[u8]) -> Result<PriorMutationOutcome, Error> {
    let value: OutcomeV1 = decode_json("mutation outcome", version, bytes)?;
    PriorMutationOutcome::try_from(value)
        .map_err(|_| malformed("mutation outcome", version, bytes.len()))
}

pub fn decode_outcome_for_operation(
    operation: MutationOperation,
    version: i64,
    bytes: &[u8],
) -> Result<PriorMutationOutcome, Error> {
    let outcome = decode_outcome(version, bytes)?;
    let payload_operation = match &outcome {
        PriorMutationOutcome::CreateWorkItem(_) => MutationOperation::CreateWorkItem,
        PriorMutationOutcome::CompleteWorkItem(_) => MutationOperation::CompleteWorkItem,
        PriorMutationOutcome::CancelWorkItem(_) => MutationOperation::CancelWorkItem,
        PriorMutationOutcome::AcknowledgeAttentionSignal(_) => {
            MutationOperation::AcknowledgeAttentionSignal
        }
        PriorMutationOutcome::IngestSourceOccurrence(_) => {
            MutationOperation::IngestSourceOccurrence
        }
        PriorMutationOutcome::CreateReminder(_) => MutationOperation::CreateReminder,
        PriorMutationOutcome::FireReminder(_) => MutationOperation::FireReminder,
        PriorMutationOutcome::AcknowledgeReminderFire(_) => {
            MutationOperation::AcknowledgeReminderFire
        }
        PriorMutationOutcome::SnoozeReminderFire(_) => MutationOperation::SnoozeReminderFire,
    };
    if payload_operation != operation {
        return Err(malformed("mutation outcome", version, bytes.len()));
    }
    Ok(outcome)
}

impl From<&SourceEntityKey> for SourceKeyV1 {
    fn from(value: &SourceEntityKey) -> Self {
        Self {
            source_kind: value.source_kind().as_str().to_string(),
            source_instance: value.source_instance().as_str().to_string(),
            external_entity_id: value.external_entity_id().as_str().to_string(),
        }
    }
}

impl TryFrom<SourceKeyV1> for SourceEntityKey {
    type Error = Error;

    fn try_from(value: SourceKeyV1) -> Result<Self, Self::Error> {
        Ok(Self::new(
            SourceKind::new(value.source_kind).map_err(|error| Error::Decode(Box::new(error)))?,
            SourceInstance::new(value.source_instance)
                .map_err(|error| Error::Decode(Box::new(error)))?,
            ExternalEntityId::new(value.external_entity_id)
                .map_err(|error| Error::Decode(Box::new(error)))?,
        ))
    }
}

impl From<&SourceOrderMode> for OrderV1 {
    fn from(value: &SourceOrderMode) -> Self {
        match value {
            SourceOrderMode::Unordered => Self::Unordered,
            SourceOrderMode::Ordered { domain, value } => Self::Ordered {
                domain: domain.as_str().to_string(),
                value: value.as_ref().map(|value| value.as_bytes().to_vec()),
            },
        }
    }
}

impl TryFrom<OrderV1> for SourceOrderMode {
    type Error = Error;

    fn try_from(value: OrderV1) -> Result<Self, Self::Error> {
        match value {
            OrderV1::Unordered => Ok(Self::Unordered),
            OrderV1::Ordered { domain, value } => Ok(Self::Ordered {
                domain: SourceComparatorDomain::new(domain)
                    .map_err(|error| Error::Decode(Box::new(error)))?,
                value: value
                    .map(NormalizedSourceOrder::new)
                    .transpose()
                    .map_err(|error| Error::Decode(Box::new(error)))?,
            }),
        }
    }
}

impl From<&WorkItem> for WorkItemV1 {
    fn from(value: &WorkItem) -> Self {
        Self {
            id: mapping::id(value.id()),
            revision: value.revision().value(),
            lifecycle: match value.lifecycle() {
                WorkItemLifecycle::Open => 0,
                WorkItemLifecycle::Completed => 1,
                WorkItemLifecycle::Cancelled => 2,
            },
            due_at: value.due_at().map(mapping::timestamp),
            scheduled_at: value.scheduled_at().map(mapping::timestamp),
            defer_until: value.defer_until().map(mapping::timestamp),
            source_link: value.source_link().map(SourceKeyV1::from),
        }
    }
}

impl TryFrom<WorkItemV1> for WorkItem {
    type Error = Error;

    fn try_from(value: WorkItemV1) -> Result<Self, Self::Error> {
        let lifecycle = match value.lifecycle {
            0 => WorkItemLifecycle::Open,
            1 => WorkItemLifecycle::Completed,
            2 => WorkItemLifecycle::Cancelled,
            _ => return Err(malformed("change event", VERSION, 0)),
        };
        Self::reconstruct(
            mapping::parse_id(&value.id)?,
            Revision::try_from(value.revision).map_err(|error| Error::Decode(Box::new(error)))?,
            lifecycle,
            value
                .due_at
                .as_deref()
                .map(mapping::parse_timestamp)
                .transpose()?,
            value
                .scheduled_at
                .as_deref()
                .map(mapping::parse_timestamp)
                .transpose()?,
            value
                .defer_until
                .as_deref()
                .map(mapping::parse_timestamp)
                .transpose()?,
            value
                .source_link
                .map(SourceEntityKey::try_from)
                .transpose()?,
        )
        .map_err(|error| Error::Decode(Box::new(error)))
    }
}

impl From<&AttentionSignal> for SignalV1 {
    fn from(value: &AttentionSignal) -> Self {
        Self {
            id: mapping::id(value.id()),
            revision: value.revision().value(),
            source_lifecycle: match value.source_lifecycle() {
                SignalSourceLifecycle::Active => 0,
                SignalSourceLifecycle::Resolved => 1,
                SignalSourceLifecycle::Expired => 2,
            },
            attention_state: match value.attention_state() {
                SignalAttentionState::Unread => 0,
                SignalAttentionState::Acknowledged => 1,
            },
            source_receipt_id: mapping::id(value.source_receipt_id()),
            source_entity_id: value.source_entity_id().map(mapping::id),
        }
    }
}

impl TryFrom<SignalV1> for AttentionSignal {
    type Error = Error;

    fn try_from(value: SignalV1) -> Result<Self, Self::Error> {
        let source_lifecycle = match value.source_lifecycle {
            0 => SignalSourceLifecycle::Active,
            1 => SignalSourceLifecycle::Resolved,
            2 => SignalSourceLifecycle::Expired,
            _ => return Err(malformed("change event", VERSION, 0)),
        };
        let attention_state = match value.attention_state {
            0 => SignalAttentionState::Unread,
            1 => SignalAttentionState::Acknowledged,
            _ => return Err(malformed("change event", VERSION, 0)),
        };
        Self::reconstruct(
            mapping::parse_id(&value.id)?,
            Revision::try_from(value.revision).map_err(|error| Error::Decode(Box::new(error)))?,
            source_lifecycle,
            attention_state,
            mapping::parse_id(&value.source_receipt_id)?,
            value
                .source_entity_id
                .as_deref()
                .map(mapping::parse_id)
                .transpose()?,
        )
        .map_err(|error| Error::Decode(Box::new(error)))
    }
}

impl From<&Reminder> for ReminderV1 {
    fn from(value: &Reminder) -> Self {
        let (target_kind, target_id) = match value.target() {
            ReminderTarget::WorkItem(id) => (0, mapping::id(id)),
            ReminderTarget::AttentionSignal(id) => (1, mapping::id(id)),
        };
        Self {
            id: mapping::id(value.id()),
            revision: value.revision().value(),
            target_kind,
            target_id,
            trigger_at: mapping::timestamp(value.trigger_at()),
            fires: value
                .fires()
                .iter()
                .map(|fire| FireV1 {
                    id: mapping::id(fire.id()),
                    trigger_at: mapping::timestamp(fire.trigger_at()),
                    state: match fire.state() {
                        ReminderFireState::Scheduled => 0,
                        ReminderFireState::Fired => 1,
                        ReminderFireState::Acknowledged => 2,
                        ReminderFireState::Snoozed => 3,
                    },
                })
                .collect(),
        }
    }
}

impl TryFrom<ReminderV1> for Reminder {
    type Error = Error;

    fn try_from(value: ReminderV1) -> Result<Self, Self::Error> {
        let target = match value.target_kind {
            0 => ReminderTarget::WorkItem(mapping::parse_id(&value.target_id)?),
            1 => ReminderTarget::AttentionSignal(mapping::parse_id(&value.target_id)?),
            _ => return Err(malformed("change event", VERSION, 0)),
        };
        let fires = value
            .fires
            .into_iter()
            .map(|fire| {
                let state = match fire.state {
                    0 => ReminderFireState::Scheduled,
                    1 => ReminderFireState::Fired,
                    2 => ReminderFireState::Acknowledged,
                    3 => ReminderFireState::Snoozed,
                    _ => return Err(malformed("change event", VERSION, 0)),
                };
                ReminderFire::reconstruct(
                    mapping::parse_id(&fire.id)?,
                    mapping::parse_timestamp(&fire.trigger_at)?,
                    state,
                )
                .map_err(|error| Error::Decode(Box::new(error)))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::reconstruct(
            mapping::parse_id(&value.id)?,
            Revision::try_from(value.revision).map_err(|error| Error::Decode(Box::new(error)))?,
            target,
            mapping::parse_timestamp(&value.trigger_at)?,
            fires,
        )
        .map_err(|error| Error::Decode(Box::new(error)))
    }
}

impl From<&SourceReceipt> for ReceiptV1 {
    fn from(value: &SourceReceipt) -> Self {
        Self {
            id: mapping::id(value.id()),
            source_kind: value.occurrence_key().source_kind().as_str().to_string(),
            source_instance: value
                .occurrence_key()
                .source_instance()
                .as_str()
                .to_string(),
            occurrence_id: value.occurrence_key().occurrence_id().as_str().to_string(),
            source_entity_key: value.source_entity_key().map(SourceKeyV1::from),
            fingerprint: mapping::fingerprint(value.fingerprint()),
            source_order: OrderV1::from(value.source_order()),
            occurred_at: mapping::timestamp(value.occurred_at()),
            ingested_at: mapping::timestamp(value.ingested_at()),
        }
    }
}

impl TryFrom<ReceiptV1> for SourceReceipt {
    type Error = Error;

    fn try_from(value: ReceiptV1) -> Result<Self, Self::Error> {
        Self::reconstruct(
            mapping::parse_id(&value.id)?,
            OccurrenceKey::new(
                SourceKind::new(value.source_kind)
                    .map_err(|error| Error::Decode(Box::new(error)))?,
                SourceInstance::new(value.source_instance)
                    .map_err(|error| Error::Decode(Box::new(error)))?,
                OccurrenceId::new(value.occurrence_id)
                    .map_err(|error| Error::Decode(Box::new(error)))?,
            ),
            value
                .source_entity_key
                .map(SourceEntityKey::try_from)
                .transpose()?,
            mapping::parse_fingerprint(&value.fingerprint)?,
            SourceOrderMode::try_from(value.source_order)?,
            mapping::parse_timestamp(&value.occurred_at)?,
            mapping::parse_timestamp(&value.ingested_at)?,
        )
        .map_err(|error| Error::Decode(Box::new(error)))
    }
}

impl From<&SourceEntity> for EntityV1 {
    fn from(value: &SourceEntity) -> Self {
        Self {
            id: mapping::id(value.id()),
            key: SourceKeyV1::from(value.key()),
            version: value.version().value(),
            latest_receipt_id: mapping::id(value.latest_receipt_id()),
            order: OrderV1::from(value.order()),
        }
    }
}

impl TryFrom<EntityV1> for SourceEntity {
    type Error = Error;

    fn try_from(value: EntityV1) -> Result<Self, Self::Error> {
        Self::reconstruct(
            mapping::parse_id(&value.id)?,
            SourceEntityKey::try_from(value.key)?,
            SourceStateVersion::try_from(value.version)
                .map_err(|error| Error::Decode(Box::new(error)))?,
            mapping::parse_id(&value.latest_receipt_id)?,
            SourceOrderMode::try_from(value.order)?,
        )
        .map_err(|error| Error::Decode(Box::new(error)))
    }
}

impl From<&AffectedView> for ViewV1 {
    fn from(value: &AffectedView) -> Self {
        match value {
            AffectedView::WorkItem { work_item } => Self::WorkItem {
                value: WorkItemV1::from(work_item),
            },
            AffectedView::AttentionSignal { attention_signal } => Self::AttentionSignal {
                value: SignalV1::from(attention_signal),
            },
            AffectedView::Reminder { reminder } => Self::Reminder {
                value: ReminderV1::from(reminder),
            },
            AffectedView::SourceReceipt { source_receipt } => Self::SourceReceipt {
                value: ReceiptV1::from(source_receipt),
            },
            AffectedView::SourceEntity { source_entity } => Self::SourceEntity {
                value: EntityV1::from(source_entity),
            },
        }
    }
}

impl TryFrom<ViewV1> for AffectedView {
    type Error = Error;

    fn try_from(value: ViewV1) -> Result<Self, Self::Error> {
        match value {
            ViewV1::WorkItem { value } => Ok(Self::WorkItem {
                work_item: WorkItem::try_from(value)?,
            }),
            ViewV1::AttentionSignal { value } => Ok(Self::AttentionSignal {
                attention_signal: AttentionSignal::try_from(value)?,
            }),
            ViewV1::Reminder { value } => Ok(Self::Reminder {
                reminder: Reminder::try_from(value)?,
            }),
            ViewV1::SourceReceipt { value } => Ok(Self::SourceReceipt {
                source_receipt: SourceReceipt::try_from(value)?,
            }),
            ViewV1::SourceEntity { value } => Ok(Self::SourceEntity {
                source_entity: SourceEntity::try_from(value)?,
            }),
        }
    }
}

impl From<InboxEntry> for InboxEntryV1 {
    fn from(value: InboxEntry) -> Self {
        match value {
            InboxEntry::WorkItem(id) => Self::WorkItem(mapping::id(id)),
            InboxEntry::AttentionSignal(id) => Self::AttentionSignal(mapping::id(id)),
            InboxEntry::ReminderFire(id) => Self::ReminderFire(mapping::id(id)),
        }
    }
}

impl TryFrom<InboxEntryV1> for InboxEntry {
    type Error = Error;

    fn try_from(value: InboxEntryV1) -> Result<Self, Self::Error> {
        match value {
            InboxEntryV1::WorkItem(id) => Ok(Self::WorkItem(mapping::parse_id(&id)?)),
            InboxEntryV1::AttentionSignal(id) => Ok(Self::AttentionSignal(mapping::parse_id(&id)?)),
            InboxEntryV1::ReminderFire(id) => Ok(Self::ReminderFire(mapping::parse_id(&id)?)),
        }
    }
}

fn common<T>(operation: u8, outcome: &CommandOutcome<T>, value: OutcomeValueV1) -> OutcomeV1 {
    OutcomeV1 {
        operation,
        cursor: outcome.cursor().value(),
        change_event_id: mapping::id(outcome.change_event_id()),
        outbox_intent_id: outcome.outbox_intent_id().map(mapping::id),
        value,
    }
}

impl From<&PriorMutationOutcome> for OutcomeV1 {
    fn from(value: &PriorMutationOutcome) -> Self {
        match value {
            PriorMutationOutcome::CreateWorkItem(outcome) => common(
                0,
                outcome,
                OutcomeValueV1::WorkItem {
                    id: mapping::id(outcome.value().id()),
                },
            ),
            PriorMutationOutcome::CompleteWorkItem(outcome) => common(
                1,
                outcome,
                OutcomeValueV1::WorkItem {
                    id: mapping::id(outcome.value().id()),
                },
            ),
            PriorMutationOutcome::CancelWorkItem(outcome) => common(
                2,
                outcome,
                OutcomeValueV1::WorkItem {
                    id: mapping::id(outcome.value().id()),
                },
            ),
            PriorMutationOutcome::AcknowledgeAttentionSignal(outcome) => common(
                3,
                outcome,
                OutcomeValueV1::AttentionSignal {
                    id: mapping::id(outcome.value().id()),
                },
            ),
            PriorMutationOutcome::IngestSourceOccurrence(outcome) => {
                let decision = match outcome.value().decision() {
                    SourceIngestionDecision::Advanced => 0,
                    SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Equal) => 1,
                    SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Older) => 2,
                    SourceIngestionDecision::ReceiptOnly(
                        ReceiptOnlyReason::MissingOrderedValue,
                    ) => 3,
                    SourceIngestionDecision::ReceiptOnly(
                        ReceiptOnlyReason::ComparatorDomainMismatch,
                    ) => 4,
                    SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Incomparable) => 5,
                };
                common(
                    4,
                    outcome,
                    OutcomeValueV1::SourceIngestion {
                        receipt_id: mapping::id(outcome.value().receipt_id()),
                        signal_id: mapping::id(outcome.value().signal_id()),
                        decision,
                    },
                )
            }
            PriorMutationOutcome::CreateReminder(outcome) => reminder_outcome(5, outcome),
            PriorMutationOutcome::FireReminder(outcome) => reminder_outcome(6, outcome),
            PriorMutationOutcome::AcknowledgeReminderFire(outcome) => reminder_outcome(7, outcome),
            PriorMutationOutcome::SnoozeReminderFire(outcome) => reminder_outcome(8, outcome),
        }
    }
}

fn reminder_outcome(operation: u8, outcome: &CommandOutcome<ReminderValue>) -> OutcomeV1 {
    common(
        operation,
        outcome,
        OutcomeValueV1::Reminder {
            reminder_id: mapping::id(outcome.value().reminder_id()),
            fire_id: mapping::id(outcome.value().fire_id()),
        },
    )
}

impl TryFrom<OutcomeV1> for PriorMutationOutcome {
    type Error = Error;

    fn try_from(value: OutcomeV1) -> Result<Self, Self::Error> {
        let cursor =
            CommitCursor::try_from(value.cursor).map_err(|error| Error::Decode(Box::new(error)))?;
        let event_id = mapping::parse_id(&value.change_event_id)?;
        let outbox = value
            .outbox_intent_id
            .as_deref()
            .map(mapping::parse_id)
            .transpose()?;
        match (value.operation, value.value) {
            (operation @ 0..=2, OutcomeValueV1::WorkItem { id }) => {
                let outcome = CommandOutcome::new(
                    CommandDisposition::Applied,
                    WorkItemValue::new(mapping::parse_id(&id)?),
                    cursor,
                    event_id,
                    outbox,
                );
                match operation {
                    0 => Ok(Self::CreateWorkItem(outcome)),
                    1 => Ok(Self::CompleteWorkItem(outcome)),
                    _ => Ok(Self::CancelWorkItem(outcome)),
                }
            }
            (3, OutcomeValueV1::AttentionSignal { id }) => {
                Ok(Self::AcknowledgeAttentionSignal(CommandOutcome::new(
                    CommandDisposition::Applied,
                    AttentionSignalValue::new(mapping::parse_id(&id)?),
                    cursor,
                    event_id,
                    outbox,
                )))
            }
            (
                4,
                OutcomeValueV1::SourceIngestion {
                    receipt_id,
                    signal_id,
                    decision,
                },
            ) => {
                let decision = match decision {
                    0 => SourceIngestionDecision::Advanced,
                    1 => SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Equal),
                    2 => SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Older),
                    3 => {
                        SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::MissingOrderedValue)
                    }
                    4 => SourceIngestionDecision::ReceiptOnly(
                        ReceiptOnlyReason::ComparatorDomainMismatch,
                    ),
                    5 => SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Incomparable),
                    _ => return Err(malformed("mutation outcome", VERSION, 0)),
                };
                Ok(Self::IngestSourceOccurrence(CommandOutcome::new(
                    CommandDisposition::Applied,
                    SourceIngestionValue::new(
                        mapping::parse_id(&receipt_id)?,
                        mapping::parse_id(&signal_id)?,
                        decision,
                    ),
                    cursor,
                    event_id,
                    outbox,
                )))
            }
            (
                operation @ 5..=8,
                OutcomeValueV1::Reminder {
                    reminder_id,
                    fire_id,
                },
            ) => {
                let outcome = CommandOutcome::new(
                    CommandDisposition::Applied,
                    ReminderValue::new(
                        mapping::parse_id(&reminder_id)?,
                        mapping::parse_id(&fire_id)?,
                    ),
                    cursor,
                    event_id,
                    outbox,
                );
                match operation {
                    5 => Ok(Self::CreateReminder(outcome)),
                    6 => Ok(Self::FireReminder(outcome)),
                    7 => Ok(Self::AcknowledgeReminderFire(outcome)),
                    _ => Ok(Self::SnoozeReminderFire(outcome)),
                }
            }
            _ => Err(malformed("mutation outcome", VERSION, 0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use attention_kernel::CanonicalFingerprint;
    use attention_kernel::InvariantError;

    fn id<T>(value: &str) -> T
    where
        T: std::str::FromStr<Err = InvariantError>,
    {
        mapping::parse_id(value).expect("fixture ID")
    }

    fn fixture_outcomes() -> Vec<PriorMutationOutcome> {
        let cursor = CommitCursor::try_from(1).expect("cursor");
        let event = id("018f0000-0000-7000-8000-000000000007");
        let outbox = Some(id("018f0000-0000-7000-8000-000000000008"));
        let work_item = WorkItemValue::new(id("018f0000-0000-7000-8000-000000000001"));
        let signal = AttentionSignalValue::new(id("018f0000-0000-7000-8000-000000000002"));
        let reminder = ReminderValue::new(
            id("018f0000-0000-7000-8000-000000000003"),
            id("018f0000-0000-7000-8000-000000000004"),
        );
        vec![
            PriorMutationOutcome::CreateWorkItem(CommandOutcome::new(
                CommandDisposition::Applied,
                work_item,
                cursor,
                event,
                outbox,
            )),
            PriorMutationOutcome::CompleteWorkItem(CommandOutcome::new(
                CommandDisposition::Applied,
                work_item,
                cursor,
                event,
                outbox,
            )),
            PriorMutationOutcome::CancelWorkItem(CommandOutcome::new(
                CommandDisposition::Applied,
                work_item,
                cursor,
                event,
                outbox,
            )),
            PriorMutationOutcome::AcknowledgeAttentionSignal(CommandOutcome::new(
                CommandDisposition::Applied,
                signal,
                cursor,
                event,
                outbox,
            )),
            PriorMutationOutcome::IngestSourceOccurrence(CommandOutcome::new(
                CommandDisposition::Applied,
                SourceIngestionValue::new(
                    id("018f0000-0000-7000-8000-000000000005"),
                    id("018f0000-0000-7000-8000-000000000002"),
                    SourceIngestionDecision::Advanced,
                ),
                cursor,
                event,
                outbox,
            )),
            PriorMutationOutcome::CreateReminder(CommandOutcome::new(
                CommandDisposition::Applied,
                reminder,
                cursor,
                event,
                outbox,
            )),
            PriorMutationOutcome::FireReminder(CommandOutcome::new(
                CommandDisposition::Applied,
                reminder,
                cursor,
                event,
                outbox,
            )),
            PriorMutationOutcome::AcknowledgeReminderFire(CommandOutcome::new(
                CommandDisposition::Applied,
                reminder,
                cursor,
                event,
                outbox,
            )),
            PriorMutationOutcome::SnoozeReminderFire(CommandOutcome::new(
                CommandDisposition::Applied,
                reminder,
                cursor,
                event,
                outbox,
            )),
        ]
    }

    #[test]
    fn all_nine_outcomes_match_immutable_fixtures_and_round_trip() {
        let fixture = include_str!("../tests/fixtures/codec/outcomes_v1.jsonl");
        let outcomes = fixture_outcomes();
        for (index, (outcome, expected)) in outcomes.iter().zip(fixture.lines()).enumerate() {
            let (version, bytes) = encode_outcome(outcome).expect("encode");
            assert_eq!(version, VERSION);
            assert_eq!(bytes, expected.as_bytes(), "outcome fixture {index}");
            assert_eq!(decode_outcome(version, &bytes).expect("decode"), *outcome);
        }
    }

    #[test]
    fn every_source_decision_and_absent_outbox_round_trip() {
        let decisions = [
            SourceIngestionDecision::Advanced,
            SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Equal),
            SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Older),
            SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::MissingOrderedValue),
            SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::ComparatorDomainMismatch),
            SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Incomparable),
        ];
        for decision in decisions {
            let outcome = PriorMutationOutcome::IngestSourceOccurrence(CommandOutcome::new(
                CommandDisposition::Applied,
                SourceIngestionValue::new(
                    id("018f0000-0000-7000-8000-000000000005"),
                    id("018f0000-0000-7000-8000-000000000002"),
                    decision,
                ),
                CommitCursor::try_from(1).expect("cursor"),
                id("018f0000-0000-7000-8000-000000000007"),
                None,
            ));
            let (version, bytes) = encode_outcome(&outcome).expect("encode");
            assert_eq!(decode_outcome(version, &bytes).expect("decode"), outcome);
        }
    }

    #[test]
    fn empty_event_effects_round_trip() {
        let time = mapping::parse_timestamp("2026-08-03T12:34:56.123456789Z").expect("time");
        let draft = ChangeEventDraft::new(
            id("018f0000-0000-7000-8000-000000000007"),
            time,
            ChangeKind::SourceOccurrenceIngested,
            Vec::new(),
            InboxEffects::default(),
        );
        let (version, bytes) = encode_event(&draft).expect("encode");
        let decoded = decode_event(
            CommitCursor::try_from(1).expect("cursor"),
            draft.id(),
            time,
            draft.kind(),
            version,
            &bytes,
        )
        .expect("decode");
        assert!(decoded.draft().affected_views().is_empty());
        assert!(decoded.draft().inbox_effects().is_empty());
    }

    #[test]
    fn complete_event_matches_fixture_and_every_kind_round_trips() {
        let time = mapping::parse_timestamp("2026-08-03T12:34:56.123456789Z").expect("time");
        let work_item = WorkItem::reconstruct(
            id("018f0000-0000-7000-8000-000000000001"),
            Revision::initial(),
            WorkItemLifecycle::Open,
            Some(time),
            None,
            None,
            None,
        )
        .expect("work item");
        let key = SourceEntityKey::new(
            SourceKind::new("linear").expect("kind"),
            SourceInstance::new("workspace").expect("instance"),
            ExternalEntityId::new("ENG-1120").expect("external"),
        );
        let order = SourceOrderMode::Ordered {
            domain: SourceComparatorDomain::new("sequence").expect("domain"),
            value: Some(NormalizedSourceOrder::new([1, 2]).expect("order")),
        };
        let receipt = SourceReceipt::reconstruct(
            id("018f0000-0000-7000-8000-000000000005"),
            OccurrenceKey::new(
                SourceKind::new("linear").expect("kind"),
                SourceInstance::new("workspace").expect("instance"),
                OccurrenceId::new("occurrence-1").expect("occurrence"),
            ),
            Some(key.clone()),
            CanonicalFingerprint::reconstruct(std::array::from_fn(|index| index as u8)),
            order.clone(),
            time,
            time,
        )
        .expect("receipt");
        let entity = SourceEntity::reconstruct(
            id("018f0000-0000-7000-8000-000000000006"),
            key,
            SourceStateVersion::initial(),
            receipt.id(),
            order,
        )
        .expect("entity");
        let signal = AttentionSignal::reconstruct(
            id("018f0000-0000-7000-8000-000000000002"),
            Revision::initial(),
            SignalSourceLifecycle::Active,
            SignalAttentionState::Unread,
            receipt.id(),
            Some(entity.id()),
        )
        .expect("signal");
        let reminder = Reminder::reconstruct(
            id("018f0000-0000-7000-8000-000000000003"),
            Revision::initial(),
            ReminderTarget::WorkItem(work_item.id()),
            time,
            vec![
                ReminderFire::reconstruct(
                    id("018f0000-0000-7000-8000-000000000004"),
                    time,
                    ReminderFireState::Scheduled,
                )
                .expect("fire"),
            ],
        )
        .expect("reminder");
        let draft = ChangeEventDraft::new(
            id("018f0000-0000-7000-8000-000000000007"),
            time,
            ChangeKind::WorkItemCreated,
            vec![
                AffectedView::WorkItem {
                    work_item: work_item.clone(),
                },
                AffectedView::AttentionSignal {
                    attention_signal: signal,
                },
                AffectedView::Reminder { reminder },
                AffectedView::SourceReceipt {
                    source_receipt: receipt,
                },
                AffectedView::SourceEntity {
                    source_entity: entity,
                },
            ],
            InboxEffects::new(
                vec![
                    InboxEntry::WorkItem(work_item.id()),
                    InboxEntry::AttentionSignal(id("018f0000-0000-7000-8000-000000000002")),
                    InboxEntry::ReminderFire(id("018f0000-0000-7000-8000-000000000004")),
                ],
                Vec::new(),
            ),
        );
        let (version, bytes) = encode_event(&draft).expect("encode");
        assert_eq!(
            bytes,
            include_bytes!("../tests/fixtures/codec/event_v1.json")
        );
        for kind in [
            ChangeKind::WorkItemCreated,
            ChangeKind::WorkItemCompleted,
            ChangeKind::WorkItemCancelled,
            ChangeKind::AttentionSignalAcknowledged,
            ChangeKind::SourceOccurrenceIngested,
            ChangeKind::ReminderCreated,
            ChangeKind::ReminderFired,
            ChangeKind::ReminderFireAcknowledged,
            ChangeKind::ReminderFireSnoozed,
        ] {
            let decoded = decode_event(
                CommitCursor::try_from(1).expect("cursor"),
                draft.id(),
                time,
                kind,
                version,
                &bytes,
            )
            .expect("decode");
            assert_eq!(decoded.draft().kind(), kind);
            assert_eq!(decoded.draft().affected_views(), draft.affected_views());
            assert_eq!(decoded.draft().inbox_effects(), draft.inbox_effects());
        }
    }

    #[test]
    fn unknown_and_malformed_versions_are_bounded_and_fail_closed() {
        let unknown = decode_outcome(2, b"secret raw payload").expect_err("unknown version");
        assert_eq!(
            unknown.to_string(),
            "persisted mutation outcome codec version 2 is unsupported (18 bytes)"
        );
        assert!(!unknown.to_string().contains("secret"));
        let malformed = decode_outcome(1, b"secret raw payload").expect_err("malformed version");
        assert_eq!(
            malformed.to_string(),
            "persisted mutation outcome codec version 1 is malformed (18 bytes)"
        );
        assert!(!malformed.to_string().contains("secret"));

        let invalid_native = br#"{"operation":0,"cursor":0,"change_event_id":"secret","outbox_intent_id":null,"value":{"type":"work_item","id":"secret"}}"#;
        let invalid = decode_outcome(1, invalid_native).expect_err("invalid native value");
        assert_eq!(
            invalid.to_string(),
            format!(
                "persisted mutation outcome codec version 1 is malformed ({} bytes)",
                invalid_native.len()
            )
        );
        assert!(!invalid.to_string().contains("secret"));
    }
}
