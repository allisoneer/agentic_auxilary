use attention_kernel as k;
use attention_protocol as p;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Timelike;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MappingError {
    #[error("invalid native value for wire protocol: {0}")]
    Invalid(String),
    #[error("event inbox addition has no complete affected view")]
    MissingAffectedView,
}
type Result<T> = std::result::Result<T, MappingError>;

macro_rules! id_to_wire {
    ($fn:ident,$native:ty,$wire:ident) => {
        pub fn $fn(value: $native) -> p::$wire {
            p::$wire(value.to_string())
        }
    };
}
id_to_wire!(work_item_id, k::WorkItemId, WorkItemId);
id_to_wire!(signal_id, k::AttentionSignalId, AttentionSignalId);
id_to_wire!(reminder_id, k::ReminderId, ReminderId);
id_to_wire!(fire_id, k::ReminderFireId, ReminderFireId);
id_to_wire!(receipt_id, k::SourceReceiptId, SourceReceiptId);
id_to_wire!(entity_id, k::SourceEntityId, SourceEntityId);
id_to_wire!(event_id, k::ChangeEventId, ChangeEventId);
id_to_wire!(intent_id, k::OutboxIntentId, OutboxIntentId);

pub fn parse_fire_id(v: &p::ReminderFireId) -> Result<k::ReminderFireId> {
    parse_id(v.as_str())
}
pub fn parse_entity_id(v: &p::SourceEntityId) -> Result<k::SourceEntityId> {
    parse_id(v.as_str())
}
pub fn parse_idempotency(v: &p::MutationIdempotencyKey) -> Result<k::MutationIdempotencyKey> {
    parse_id(v.as_str())
}
pub fn parse_revision(v: &p::Revision) -> Result<k::Revision> {
    let value = v
        .as_str()
        .parse::<u64>()
        .map_err(|e| MappingError::Invalid(e.to_string()))?;
    k::Revision::try_from(value).map_err(|e| MappingError::Invalid(e.to_string()))
}
fn parse_time(v: &p::WireTimestamp) -> chrono::DateTime<chrono::Utc> {
    *v.as_datetime()
}
fn parse_id<T: FromStr>(value: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|e| MappingError::Invalid(e.to_string()))
}
pub fn parse_work_item_id(v: &p::WorkItemId) -> Result<k::WorkItemId> {
    parse_id(v.as_str())
}
pub fn parse_signal_id(v: &p::AttentionSignalId) -> Result<k::AttentionSignalId> {
    parse_id(v.as_str())
}
pub fn parse_reminder_id(v: &p::ReminderId) -> Result<k::ReminderId> {
    parse_id(v.as_str())
}
pub fn parse_receipt_id(v: &p::SourceReceiptId) -> Result<k::SourceReceiptId> {
    parse_id(v.as_str())
}
pub fn parse_intent_id(v: &p::OutboxIntentId) -> Result<k::OutboxIntentId> {
    parse_id(v.as_str())
}
pub fn parse_delivery_token(v: &p::DeliveryLeaseToken) -> Result<k::DeliveryLeaseToken> {
    let bytes = URL_SAFE_NO_PAD
        .decode(v.as_str())
        .map_err(|e| MappingError::Invalid(e.to_string()))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| MappingError::Invalid("delivery lease token length".into()))?;
    Ok(k::DeliveryLeaseToken::from_bytes(bytes))
}
pub fn delivery_token(v: k::DeliveryLeaseToken) -> Result<p::DeliveryLeaseToken> {
    p::DeliveryLeaseToken::parse(URL_SAFE_NO_PAD.encode(v.as_bytes()))
        .map_err(|e| MappingError::Invalid(e.to_string()))
}
pub fn parse_wire_time(v: &p::WireTimestamp) -> chrono::DateTime<chrono::Utc> {
    parse_time(v)
}
pub fn wire_time(v: &chrono::DateTime<chrono::Utc>) -> Result<p::WireTimestamp> {
    time(v)
}
pub fn delivery_authority(v: &k::DeliveryAuthority) -> Result<p::DeliveryAuthorityView> {
    let intent = v.intent();
    let subject = match intent.subject() {
        k::DeliverySubject::AttentionSignal(id) => p::DeliverySubject::AttentionSignal {
            attention_signal_id: signal_id(id),
        },
        k::DeliverySubject::ReminderFire(id) => p::DeliverySubject::ReminderFire {
            reminder_fire_id: fire_id(id),
        },
    };
    let status = match v.state().status() {
        k::DeliveryStatus::Pending => p::DeliveryStateView::Pending,
        k::DeliveryStatus::Leased { expires_at, .. } => p::DeliveryStateView::Leased {
            expires_at: time(expires_at)?,
        },
        k::DeliveryStatus::Retryable {
            attempt,
            error,
            next_retry_at,
        } => p::DeliveryStateView::Retryable {
            attempt: *attempt,
            error: error.as_str().into(),
            next_retry_at: time(next_retry_at)?,
        },
        k::DeliveryStatus::Succeeded {
            provider_message_id,
            succeeded_at,
        } => p::DeliveryStateView::Succeeded {
            provider_message_id: p::ProviderMessageId(provider_message_id.as_str().into()),
            succeeded_at: time(succeeded_at)?,
        },
        k::DeliveryStatus::Skipped { reason, skipped_at } => p::DeliveryStateView::Skipped {
            reason: reason.as_str().into(),
            skipped_at: time(skipped_at)?,
        },
        k::DeliveryStatus::TerminalFailure {
            attempt,
            error,
            failed_at,
        } => p::DeliveryStateView::TerminalFailure {
            attempt: *attempt,
            error: error.as_str().into(),
            failed_at: time(failed_at)?,
        },
    };
    Ok(p::DeliveryAuthorityView {
        intent: p::OutboxIntentView {
            id: intent_id(intent.id()),
            subject,
            originating_change_event_id: event_id(intent.originating_change_event_id()),
            created_at: time(intent.created_at())?,
            purpose: match intent.purpose() {
                k::DeliveryPurpose::FreshAttention => p::DeliveryPurpose::FreshAttention,
                k::DeliveryPurpose::ReminderFired => p::DeliveryPurpose::ReminderFired,
            },
        },
        state: status,
    })
}
pub fn cursor(v: k::CommitCursor) -> p::Cursor {
    p::Cursor(v.value().to_string())
}
pub fn parse_cursor(v: &p::Cursor) -> Result<k::CommitCursor> {
    let n = v
        .as_str()
        .parse::<u64>()
        .map_err(|e| MappingError::Invalid(e.to_string()))?;
    k::CommitCursor::try_from(n).map_err(|e| MappingError::Invalid(e.to_string()))
}
pub fn revision(v: k::Revision) -> Result<p::Revision> {
    p::Revision::parse(v.value().to_string()).map_err(|e| MappingError::Invalid(e.to_string()))
}
pub fn version(v: k::SourceStateVersion) -> Result<p::SourceStateVersion> {
    p::SourceStateVersion::parse(v.value().to_string())
        .map_err(|e| MappingError::Invalid(e.to_string()))
}
fn time(v: &chrono::DateTime<chrono::Utc>) -> Result<p::WireTimestamp> {
    p::WireTimestamp::try_from(*v).map_err(|e| MappingError::Invalid(e.to_string()))
}
pub fn source_key(v: &k::SourceEntityKey) -> p::SourceEntityKey {
    p::SourceEntityKey {
        source_kind: p::SourceKind(v.source_kind().as_str().into()),
        source_instance: p::SourceInstance(v.source_instance().as_str().into()),
        external_entity_id: p::ExternalEntityId(v.external_entity_id().as_str().into()),
    }
}
pub fn parse_source_key(v: &p::SourceEntityKey) -> Result<k::SourceEntityKey> {
    validate_source_parts(&[
        v.source_kind.as_str(),
        v.source_instance.as_str(),
        v.external_entity_id.as_str(),
    ])?;
    let map = |e: k::InvariantError| MappingError::Invalid(e.to_string());
    Ok(k::SourceEntityKey::new(
        k::SourceKind::new(v.source_kind.as_str()).map_err(map)?,
        k::SourceInstance::new(v.source_instance.as_str()).map_err(map)?,
        k::ExternalEntityId::new(v.external_entity_id.as_str()).map_err(map)?,
    ))
}

const MAX_SOURCE_COMPONENT_BYTES: usize = 256;
const MAX_SOURCE_ORDER_BYTES: usize = 4096;
fn validate_source_parts(parts: &[&str]) -> Result<()> {
    if parts
        .iter()
        .any(|part| part.len() > MAX_SOURCE_COMPONENT_BYTES)
    {
        return Err(MappingError::Invalid(
            "source component exceeds bound".into(),
        ));
    }
    Ok(())
}
fn parse_occurrence(v: &p::OccurrenceKey) -> Result<k::OccurrenceKey> {
    validate_source_parts(&[
        v.source_kind.as_str(),
        v.source_instance.as_str(),
        v.occurrence_id.as_str(),
    ])?;
    let map = |e: k::InvariantError| MappingError::Invalid(e.to_string());
    Ok(k::OccurrenceKey::new(
        k::SourceKind::new(v.source_kind.as_str()).map_err(map)?,
        k::SourceInstance::new(v.source_instance.as_str()).map_err(map)?,
        k::OccurrenceId::new(v.occurrence_id.as_str()).map_err(map)?,
    ))
}
fn parse_order(v: &p::SourceOrder) -> Result<k::SourceOrderMode> {
    Ok(match v {
        p::SourceOrder::Unordered => k::SourceOrderMode::Unordered,
        p::SourceOrder::Ordered { domain, value } => {
            validate_source_parts(&[domain.as_str()])?;
            let bytes = value
                .as_ref()
                .map(|value| {
                    URL_SAFE_NO_PAD
                        .decode(value.as_str())
                        .map_err(|e| MappingError::Invalid(e.to_string()))
                })
                .transpose()?;
            if bytes
                .as_ref()
                .is_some_and(|bytes| bytes.len() > MAX_SOURCE_ORDER_BYTES)
            {
                return Err(MappingError::Invalid("source order exceeds bound".into()));
            }
            k::SourceOrderMode::Ordered {
                domain: k::SourceComparatorDomain::new(domain.as_str())
                    .map_err(|e| MappingError::Invalid(e.to_string()))?,
                value: bytes
                    .as_deref()
                    .map(k::NormalizedSourceOrder::new)
                    .transpose()
                    .map_err(|e| MappingError::Invalid(e.to_string()))?,
            }
        }
    })
}

pub fn create_work_item_command(v: &p::CreateWorkItemParams) -> Result<k::CreateWorkItem> {
    Ok(k::CreateWorkItem::new(
        parse_work_item_id(&v.id)?,
        v.due_at.as_ref().map(parse_time),
        v.scheduled_at.as_ref().map(parse_time),
        v.defer_until.as_ref().map(parse_time),
        v.source_link.as_ref().map(parse_source_key).transpose()?,
        parse_idempotency(&v.idempotency_key)?,
    ))
}
pub fn complete_work_item_command(v: &p::CompleteWorkItemParams) -> Result<k::CompleteWorkItem> {
    Ok(k::CompleteWorkItem::new(
        parse_work_item_id(&v.id)?,
        parse_revision(&v.expected_revision)?,
        parse_idempotency(&v.idempotency_key)?,
    ))
}
pub fn cancel_work_item_command(v: &p::CancelWorkItemParams) -> Result<k::CancelWorkItem> {
    Ok(k::CancelWorkItem::new(
        parse_work_item_id(&v.id)?,
        parse_revision(&v.expected_revision)?,
        parse_idempotency(&v.idempotency_key)?,
    ))
}
pub fn acknowledge_signal_command(
    v: &p::AcknowledgeAttentionSignalParams,
) -> Result<k::AcknowledgeAttentionSignal> {
    Ok(k::AcknowledgeAttentionSignal::new(
        parse_signal_id(&v.id)?,
        parse_revision(&v.expected_revision)?,
        parse_idempotency(&v.idempotency_key)?,
    ))
}
pub fn ingest_source_command(
    v: &p::IngestSourceOccurrenceParams,
) -> Result<k::IngestSourceOccurrence> {
    let entity = v
        .entity
        .as_ref()
        .map(|entity| {
            Ok(k::SourceEntityIdentity::new(
                parse_entity_id(&entity.id)?,
                parse_source_key(&entity.key)?,
            ))
        })
        .transpose()?;
    Ok(k::IngestSourceOccurrence::new(
        parse_receipt_id(&v.receipt_id)?,
        entity,
        parse_signal_id(&v.signal_id)?,
        parse_occurrence(&v.occurrence_key)?,
        parse_time(&v.occurred_at),
        {
            let now = chrono::Utc::now();
            now.with_nanosecond((now.timestamp_subsec_nanos() / 1_000) * 1_000)
                .unwrap_or(now)
        },
        parse_order(&v.order)?,
        match v.source_lifecycle {
            p::SignalSourceLifecycle::Active => k::SignalSourceLifecycle::Active,
            p::SignalSourceLifecycle::Resolved => k::SignalSourceLifecycle::Resolved,
            p::SignalSourceLifecycle::Expired => k::SignalSourceLifecycle::Expired,
        },
        v.fresh_attention,
        parse_idempotency(&v.idempotency_key)?,
    ))
}
pub fn create_reminder_command(v: &p::CreateReminderParams) -> Result<k::CreateReminder> {
    let target = match &v.target {
        p::ReminderTarget::WorkItem { work_item_id } => {
            k::ReminderTarget::WorkItem(parse_work_item_id(work_item_id)?)
        }
        p::ReminderTarget::AttentionSignal {
            attention_signal_id,
        } => k::ReminderTarget::AttentionSignal(parse_signal_id(attention_signal_id)?),
    };
    Ok(k::CreateReminder::new(
        parse_reminder_id(&v.reminder_id)?,
        parse_fire_id(&v.initial_fire_id)?,
        target,
        parse_time(&v.trigger_at),
        parse_idempotency(&v.idempotency_key)?,
    ))
}
pub fn acknowledge_reminder_command(
    v: &p::AcknowledgeReminderFireParams,
) -> Result<k::AcknowledgeReminderFire> {
    Ok(k::AcknowledgeReminderFire::new(
        parse_reminder_id(&v.reminder_id)?,
        parse_fire_id(&v.fire_id)?,
        parse_revision(&v.expected_revision)?,
        parse_idempotency(&v.idempotency_key)?,
    ))
}
pub fn snooze_reminder_command(v: &p::SnoozeReminderFireParams) -> Result<k::SnoozeReminderFire> {
    Ok(k::SnoozeReminderFire::new(
        parse_reminder_id(&v.reminder_id)?,
        parse_fire_id(&v.fire_id)?,
        parse_fire_id(&v.replacement_fire_id)?,
        parse_time(&v.replacement_trigger_at),
        parse_revision(&v.expected_revision)?,
        parse_idempotency(&v.idempotency_key)?,
    ))
}

fn disposition(v: k::CommandDisposition) -> p::MutationDisposition {
    match v {
        k::CommandDisposition::Applied => p::MutationDisposition::Applied,
        k::CommandDisposition::Replayed => p::MutationDisposition::Replayed,
    }
}
fn mutation_result<T, U>(v: &k::CommandOutcome<T>, value: U) -> p::MutationResult<U> {
    p::MutationResult {
        disposition: disposition(v.disposition()),
        value,
        cursor: cursor(v.cursor()),
        change_event_id: event_id(v.change_event_id()),
        outbox_intent_id: v
            .outbox_intent_id()
            .map(|id| p::OutboxIntentId(id.to_string())),
    }
}
pub fn work_item_result(v: &k::CreateWorkItemResult) -> p::CreateWorkItemResult {
    mutation_result(
        v,
        p::WorkItemMutationValue {
            id: work_item_id(v.value().id()),
        },
    )
}
pub fn signal_result(
    v: &k::AcknowledgeAttentionSignalResult,
) -> p::AcknowledgeAttentionSignalResult {
    mutation_result(
        v,
        p::AttentionSignalMutationValue {
            id: signal_id(v.value().id()),
        },
    )
}
pub fn reminder_result(v: &k::CreateReminderResult) -> p::CreateReminderResult {
    mutation_result(
        v,
        p::ReminderMutationValue {
            reminder_id: reminder_id(v.value().reminder_id()),
            fire_id: fire_id(v.value().fire_id()),
        },
    )
}
pub fn source_result(v: &k::IngestSourceOccurrenceResult) -> p::IngestSourceOccurrenceResult {
    let decision = match v.value().decision() {
        k::SourceIngestionDecision::Advanced => p::SourceIngestionDecision::Advanced,
        k::SourceIngestionDecision::ReceiptOnly(reason) => {
            p::SourceIngestionDecision::ReceiptOnly {
                reason: match reason {
                    k::ReceiptOnlyReason::Equal => p::ReceiptOnlyReason::Equal,
                    k::ReceiptOnlyReason::Older => p::ReceiptOnlyReason::Older,
                    k::ReceiptOnlyReason::MissingOrderedValue => {
                        p::ReceiptOnlyReason::MissingOrderedValue
                    }
                    k::ReceiptOnlyReason::ComparatorDomainMismatch => {
                        p::ReceiptOnlyReason::ComparatorDomainMismatch
                    }
                    k::ReceiptOnlyReason::Incomparable => p::ReceiptOnlyReason::Incomparable,
                },
            }
        }
    };
    mutation_result(
        v,
        p::SourceIngestionValue {
            receipt_id: receipt_id(v.value().receipt_id()),
            signal_id: signal_id(v.value().signal_id()),
            decision,
        },
    )
}

pub fn occurrence(v: &k::OccurrenceKey) -> p::OccurrenceKey {
    p::OccurrenceKey {
        source_kind: p::SourceKind(v.source_kind().as_str().into()),
        source_instance: p::SourceInstance(v.source_instance().as_str().into()),
        occurrence_id: p::OccurrenceId(v.occurrence_id().as_str().into()),
    }
}
fn order(v: &k::SourceOrderMode) -> Result<p::SourceOrder> {
    Ok(match v {
        k::SourceOrderMode::Unordered => p::SourceOrder::Unordered,
        k::SourceOrderMode::Ordered { domain, value } => p::SourceOrder::Ordered {
            domain: p::SourceOrderDomain(domain.as_str().into()),
            value: value
                .as_ref()
                .map(|x| p::NormalizedSourceOrder::parse(URL_SAFE_NO_PAD.encode(x.as_bytes())))
                .transpose()
                .map_err(|e| MappingError::Invalid(e.to_string()))?,
        },
    })
}
fn target(v: k::ReminderTarget) -> p::ReminderTarget {
    match v {
        k::ReminderTarget::WorkItem(id) => p::ReminderTarget::WorkItem {
            work_item_id: work_item_id(id),
        },
        k::ReminderTarget::AttentionSignal(id) => p::ReminderTarget::AttentionSignal {
            attention_signal_id: signal_id(id),
        },
    }
}
pub fn work_item(v: &k::WorkItem) -> Result<p::WorkItemView> {
    Ok(p::WorkItemView {
        id: work_item_id(v.id()),
        revision: revision(v.revision())?,
        lifecycle: match v.lifecycle() {
            k::WorkItemLifecycle::Open => p::WorkItemLifecycle::Open,
            k::WorkItemLifecycle::Completed => p::WorkItemLifecycle::Completed,
            k::WorkItemLifecycle::Cancelled => p::WorkItemLifecycle::Cancelled,
        },
        due_at: v.due_at().map(time).transpose()?,
        scheduled_at: v.scheduled_at().map(time).transpose()?,
        defer_until: v.defer_until().map(time).transpose()?,
        source_link: v.source_link().map(source_key),
    })
}
pub fn signal(v: &k::AttentionSignal) -> Result<p::AttentionSignalView> {
    Ok(p::AttentionSignalView {
        id: signal_id(v.id()),
        revision: revision(v.revision())?,
        source_lifecycle: match v.source_lifecycle() {
            k::SignalSourceLifecycle::Active => p::SignalSourceLifecycle::Active,
            k::SignalSourceLifecycle::Resolved => p::SignalSourceLifecycle::Resolved,
            k::SignalSourceLifecycle::Expired => p::SignalSourceLifecycle::Expired,
        },
        attention_state: match v.attention_state() {
            k::SignalAttentionState::Unread => p::SignalAttentionState::Unread,
            k::SignalAttentionState::Acknowledged => p::SignalAttentionState::Acknowledged,
        },
        source_receipt_id: receipt_id(v.source_receipt_id()),
        source_entity_id: v.source_entity_id().map(entity_id),
    })
}
fn fire(v: &k::ReminderFire) -> Result<p::ReminderFireView> {
    Ok(p::ReminderFireView {
        id: fire_id(v.id()),
        trigger_at: time(v.trigger_at())?,
        state: match v.state() {
            k::ReminderFireState::Scheduled => p::ReminderFireState::Scheduled,
            k::ReminderFireState::Fired => p::ReminderFireState::Fired,
            k::ReminderFireState::Acknowledged => p::ReminderFireState::Acknowledged,
            k::ReminderFireState::Snoozed => p::ReminderFireState::Snoozed,
        },
    })
}
pub fn reminder(v: &k::Reminder) -> Result<p::ReminderView> {
    Ok(p::ReminderView {
        id: reminder_id(v.id()),
        revision: revision(v.revision())?,
        target: target(v.target()),
        trigger_at: time(v.trigger_at())?,
        fires: v.fires().iter().map(fire).collect::<Result<_>>()?,
    })
}
pub fn receipt(v: &k::SourceReceipt) -> Result<p::SourceReceiptView> {
    Ok(p::SourceReceiptView {
        id: receipt_id(v.id()),
        occurrence_key: occurrence(v.occurrence_key()),
        source_entity_key: v.source_entity_key().map(source_key),
        source_order: order(v.source_order())?,
        occurred_at: time(v.occurred_at())?,
        ingested_at: time(v.ingested_at())?,
    })
}
pub fn entity(v: &k::SourceEntity) -> Result<p::SourceEntityView> {
    Ok(p::SourceEntityView {
        id: entity_id(v.id()),
        key: source_key(v.key()),
        version: version(v.version())?,
        latest_receipt_id: receipt_id(v.latest_receipt_id()),
        order: order(v.order())?,
    })
}
fn affected(v: &k::AffectedView) -> Result<p::AffectedView> {
    Ok(match v {
        k::AffectedView::WorkItem { work_item: v } => p::AffectedView::WorkItem {
            work_item: work_item(v)?,
        },
        k::AffectedView::AttentionSignal {
            attention_signal: v,
        } => p::AffectedView::AttentionSignal {
            attention_signal: signal(v)?,
        },
        k::AffectedView::Reminder { reminder: v } => p::AffectedView::Reminder {
            reminder: reminder(v)?,
        },
        k::AffectedView::SourceReceipt { source_receipt: v } => p::AffectedView::SourceReceipt {
            source_receipt: receipt(v)?,
        },
        k::AffectedView::SourceEntity { source_entity: v } => p::AffectedView::SourceEntity {
            source_entity: entity(v)?,
        },
    })
}
fn addition(entry: &k::InboxEntry, views: &[k::AffectedView]) -> Result<p::InboxEntryView> {
    match entry {
        k::InboxEntry::WorkItem(id) => views
            .iter()
            .find_map(|v| {
                if let k::AffectedView::WorkItem { work_item: v } = v {
                    (v.id() == *id).then_some(v)
                } else {
                    None
                }
            })
            .ok_or(MappingError::MissingAffectedView)
            .and_then(|v| {
                Ok(p::InboxEntryView::WorkItem {
                    work_item: work_item(v)?,
                })
            }),
        k::InboxEntry::AttentionSignal(id) => views
            .iter()
            .find_map(|v| {
                if let k::AffectedView::AttentionSignal {
                    attention_signal: v,
                } = v
                {
                    (v.id() == *id).then_some(v)
                } else {
                    None
                }
            })
            .ok_or(MappingError::MissingAffectedView)
            .and_then(|v| {
                Ok(p::InboxEntryView::AttentionSignal {
                    attention_signal: signal(v)?,
                })
            }),
        k::InboxEntry::ReminderFire(id) => {
            let r = views
                .iter()
                .find_map(|v| {
                    if let k::AffectedView::Reminder { reminder: v } = v {
                        v.fires().iter().any(|f| f.id() == *id).then_some(v)
                    } else {
                        None
                    }
                })
                .ok_or(MappingError::MissingAffectedView)?;
            let f = r
                .fires()
                .iter()
                .find(|f| f.id() == *id)
                .ok_or(MappingError::MissingAffectedView)?;
            Ok(p::InboxEntryView::ReminderFire {
                reminder_id: reminder_id(r.id()),
                reminder_revision: revision(r.revision())?,
                target: target(r.target()),
                fire: fire(f)?,
            })
        }
    }
}
fn removal(v: &k::InboxEntry) -> p::InboxEntryKey {
    match v {
        k::InboxEntry::WorkItem(id) => p::InboxEntryKey::WorkItem {
            work_item_id: work_item_id(*id),
        },
        k::InboxEntry::AttentionSignal(id) => p::InboxEntryKey::AttentionSignal {
            attention_signal_id: signal_id(*id),
        },
        k::InboxEntry::ReminderFire(id) => p::InboxEntryKey::ReminderFire {
            reminder_fire_id: fire_id(*id),
        },
    }
}
pub fn event(v: &k::ChangeEvent) -> Result<p::ChangeEvent> {
    let d = v.draft();
    Ok(p::ChangeEvent {
        id: event_id(d.id()),
        cursor: cursor(v.cursor()),
        occurred_at: time(d.occurred_at())?,
        kind: match d.kind() {
            k::ChangeKind::WorkItemCreated => p::ChangeKind::WorkItemCreated,
            k::ChangeKind::WorkItemCompleted => p::ChangeKind::WorkItemCompleted,
            k::ChangeKind::WorkItemCancelled => p::ChangeKind::WorkItemCancelled,
            k::ChangeKind::AttentionSignalAcknowledged => {
                p::ChangeKind::AttentionSignalAcknowledged
            }
            k::ChangeKind::SourceOccurrenceIngested => p::ChangeKind::SourceOccurrenceIngested,
            k::ChangeKind::ReminderCreated => p::ChangeKind::ReminderCreated,
            k::ChangeKind::ReminderFired => p::ChangeKind::ReminderFired,
            k::ChangeKind::ReminderFireAcknowledged => p::ChangeKind::ReminderFireAcknowledged,
            k::ChangeKind::ReminderFireSnoozed => p::ChangeKind::ReminderFireSnoozed,
        },
        affected: d
            .affected_views()
            .iter()
            .map(affected)
            .collect::<Result<_>>()?,
        inbox: p::InboxEffects {
            upserts: d
                .inbox_effects()
                .additions()
                .iter()
                .map(|e| addition(e, d.affected_views()))
                .collect::<Result<_>>()?,
            removals: d.inbox_effects().removals().iter().map(removal).collect(),
        },
    })
}
pub fn snapshot(v: &k::AttentionSnapshot) -> Result<p::AttentionSnapshot> {
    let work_items = v
        .work_items()
        .iter()
        .map(work_item)
        .collect::<Result<Vec<_>>>()?;
    let attention_signals = v.signals().iter().map(signal).collect::<Result<Vec<_>>>()?;
    let reminders = v
        .reminders()
        .iter()
        .map(reminder)
        .collect::<Result<Vec<_>>>()?;
    let mut inbox = Vec::new();
    for x in v.work_items().iter().filter(|x| x.is_in_default_inbox()) {
        inbox.push(p::InboxEntryView::WorkItem {
            work_item: work_item(x)?,
        });
    }
    for x in v.signals().iter().filter(|x| x.is_in_default_inbox()) {
        inbox.push(p::InboxEntryView::AttentionSignal {
            attention_signal: signal(x)?,
        });
    }
    for r in v.reminders() {
        for f in r.fires().iter().filter(|x| x.is_in_default_inbox()) {
            inbox.push(p::InboxEntryView::ReminderFire {
                reminder_id: reminder_id(r.id()),
                reminder_revision: revision(r.revision())?,
                target: target(r.target()),
                fire: fire(f)?,
            });
        }
    }
    Ok(p::AttentionSnapshot {
        work_items,
        attention_signals,
        reminders,
        inbox,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use chrono::Utc;
    use serde_json::Value;
    use serde_json::json;

    fn at(hour: u32) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&format!("2026-08-13T{hour:02}:00:00Z"))
            .expect("valid fixture timestamp")
            .with_timezone(&Utc)
    }
    fn revision(value: u64) -> k::Revision {
        k::Revision::try_from(value).expect("nonzero revision")
    }
    fn source_key() -> k::SourceEntityKey {
        k::SourceEntityKey::new(
            k::SourceKind::new("linear").expect("kind"),
            k::SourceInstance::new("primary").expect("instance"),
            k::ExternalEntityId::new("ENG-1110").expect("external id"),
        )
    }
    fn occurrence_key() -> k::OccurrenceKey {
        k::OccurrenceKey::new(
            k::SourceKind::new("linear").expect("kind"),
            k::SourceInstance::new("primary").expect("instance"),
            k::OccurrenceId::new("webhook-42").expect("occurrence"),
        )
    }
    fn work_item(lifecycle: k::WorkItemLifecycle) -> k::WorkItem {
        k::WorkItem::reconstruct(
            k::WorkItemId::new(),
            revision(3),
            lifecycle,
            Some(at(13)),
            Some(at(11)),
            Some(at(12)),
            Some(source_key()),
        )
        .expect("work item")
    }
    fn signal(
        lifecycle: k::SignalSourceLifecycle,
        attention: k::SignalAttentionState,
    ) -> k::AttentionSignal {
        k::AttentionSignal::reconstruct(
            k::AttentionSignalId::new(),
            revision(4),
            lifecycle,
            attention,
            k::SourceReceiptId::new(),
            Some(k::SourceEntityId::new()),
        )
        .expect("signal")
    }
    fn reminder(states: &[k::ReminderFireState]) -> k::Reminder {
        let fires = states
            .iter()
            .enumerate()
            .map(|(index, state)| {
                k::ReminderFire::reconstruct(
                    k::ReminderFireId::new(),
                    at(14 + u32::try_from(index).expect("small index")),
                    *state,
                )
                .expect("fire")
            })
            .collect();
        k::Reminder::reconstruct(
            k::ReminderId::new(),
            revision(5),
            k::ReminderTarget::WorkItem(k::WorkItemId::new()),
            at(14),
            fires,
        )
        .expect("reminder")
    }
    fn ordered(value: Option<&[u8]>) -> k::SourceOrderMode {
        k::SourceOrderMode::Ordered {
            domain: k::SourceComparatorDomain::new("sequence").expect("domain"),
            value: value.map(|value| k::NormalizedSourceOrder::new(value).expect("order")),
        }
    }
    fn receipt(order: k::SourceOrderMode) -> k::SourceReceipt {
        k::SourceReceipt::reconstruct(
            k::SourceReceiptId::new(),
            occurrence_key(),
            Some(source_key()),
            k::CanonicalFingerprint::reconstruct([7; 32]),
            order,
            at(10),
            at(11),
        )
        .expect("receipt")
    }
    fn entity(order: k::SourceOrderMode) -> k::SourceEntity {
        k::SourceEntity::reconstruct(
            k::SourceEntityId::new(),
            source_key(),
            k::SourceStateVersion::try_from(9).expect("version"),
            k::SourceReceiptId::new(),
            order,
        )
        .expect("entity")
    }
    fn committed(
        kind: k::ChangeKind,
        affected: Vec<k::AffectedView>,
        inbox: k::InboxEffects,
    ) -> k::ChangeEvent {
        k::ChangeEventDraft::new(k::ChangeEventId::new(), at(12), kind, affected, inbox)
            .commit(k::CommitCursor::try_from(7).expect("cursor"))
    }

    #[test]
    fn every_change_kind_maps_to_the_frozen_wire_name() {
        let cases = [
            (k::ChangeKind::WorkItemCreated, "work_item_created"),
            (k::ChangeKind::WorkItemCompleted, "work_item_completed"),
            (k::ChangeKind::WorkItemCancelled, "work_item_cancelled"),
            (
                k::ChangeKind::AttentionSignalAcknowledged,
                "attention_signal_acknowledged",
            ),
            (
                k::ChangeKind::SourceOccurrenceIngested,
                "source_occurrence_ingested",
            ),
            (k::ChangeKind::ReminderCreated, "reminder_created"),
            (k::ChangeKind::ReminderFired, "reminder_fired"),
            (
                k::ChangeKind::ReminderFireAcknowledged,
                "reminder_fire_acknowledged",
            ),
            (k::ChangeKind::ReminderFireSnoozed, "reminder_fire_snoozed"),
        ];
        for (native, expected) in cases {
            let wire =
                event(&committed(native, vec![], k::InboxEffects::default())).expect("mapping");
            assert_eq!(serde_json::to_value(wire.kind).expect("json"), expected);
        }
    }

    #[test]
    fn all_affected_views_and_realistic_source_ingress_map() {
        let order = ordered(Some(&[0, 255, 1]));
        let views = vec![
            k::AffectedView::WorkItem {
                work_item: work_item(k::WorkItemLifecycle::Open),
            },
            k::AffectedView::AttentionSignal {
                attention_signal: signal(
                    k::SignalSourceLifecycle::Resolved,
                    k::SignalAttentionState::Unread,
                ),
            },
            k::AffectedView::Reminder {
                reminder: reminder(&[k::ReminderFireState::Fired]),
            },
            k::AffectedView::SourceReceipt {
                source_receipt: receipt(order.clone()),
            },
            k::AffectedView::SourceEntity {
                source_entity: entity(order),
            },
        ];
        let value = serde_json::to_value(
            event(&committed(
                k::ChangeKind::SourceOccurrenceIngested,
                views,
                k::InboxEffects::default(),
            ))
            .expect("mapping"),
        )
        .expect("json");
        let kinds = value["affected"]
            .as_array()
            .expect("affected")
            .iter()
            .map(|view| view["kind"].as_str().expect("kind"))
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                "work_item",
                "attention_signal",
                "reminder",
                "source_receipt",
                "source_entity"
            ]
        );
        assert_eq!(
            value["affected"][3]["source_receipt"]["source_order"]["value"],
            "AP8B"
        );
        assert_eq!(value["affected"][4]["source_entity"]["version"], "9");
    }

    #[test]
    fn inbox_upserts_and_removals_cover_all_three_variants() {
        let work = work_item(k::WorkItemLifecycle::Open);
        let signal = signal(
            k::SignalSourceLifecycle::Active,
            k::SignalAttentionState::Unread,
        );
        let reminder = reminder(&[k::ReminderFireState::Fired]);
        let entries = vec![
            k::InboxEntry::WorkItem(work.id()),
            k::InboxEntry::AttentionSignal(signal.id()),
            k::InboxEntry::ReminderFire(reminder.fires()[0].id()),
        ];
        let value = serde_json::to_value(
            event(&committed(
                k::ChangeKind::ReminderFired,
                vec![
                    k::AffectedView::WorkItem { work_item: work },
                    k::AffectedView::AttentionSignal {
                        attention_signal: signal,
                    },
                    k::AffectedView::Reminder { reminder },
                ],
                k::InboxEffects::new(entries.clone(), entries),
            ))
            .expect("mapping"),
        )
        .expect("json");
        for branch in ["upserts", "removals"] {
            let kinds = value["inbox"][branch]
                .as_array()
                .expect("effects")
                .iter()
                .map(|entry| entry["kind"].as_str().expect("kind"))
                .collect::<Vec<_>>();
            assert_eq!(kinds, ["work_item", "attention_signal", "reminder_fire"]);
        }
    }

    #[test]
    fn every_inbox_upsert_requires_its_matching_affected_view() {
        for entry in [
            k::InboxEntry::WorkItem(k::WorkItemId::new()),
            k::InboxEntry::AttentionSignal(k::AttentionSignalId::new()),
            k::InboxEntry::ReminderFire(k::ReminderFireId::new()),
        ] {
            let error = event(&committed(
                k::ChangeKind::WorkItemCreated,
                vec![],
                k::InboxEffects::new(vec![entry], vec![]),
            ))
            .expect_err("missing view");
            assert!(matches!(error, MappingError::MissingAffectedView));
        }
    }

    #[test]
    fn snapshot_maps_lifecycle_combinations_and_exact_inbox_predicates() {
        let work_items = [
            k::WorkItemLifecycle::Open,
            k::WorkItemLifecycle::Completed,
            k::WorkItemLifecycle::Cancelled,
        ]
        .map(work_item)
        .to_vec();
        let signals = [
            k::SignalSourceLifecycle::Active,
            k::SignalSourceLifecycle::Resolved,
            k::SignalSourceLifecycle::Expired,
        ]
        .into_iter()
        .flat_map(|lifecycle| {
            [
                signal(lifecycle, k::SignalAttentionState::Unread),
                signal(lifecycle, k::SignalAttentionState::Acknowledged),
            ]
        })
        .collect();
        let reminders = vec![
            reminder(&[
                k::ReminderFireState::Acknowledged,
                k::ReminderFireState::Snoozed,
                k::ReminderFireState::Fired,
            ]),
            reminder(&[k::ReminderFireState::Scheduled]),
        ];
        let wire = snapshot(&k::AttentionSnapshot::new(
            k::CommitCursor::try_from(1).expect("cursor"),
            work_items,
            signals,
            reminders,
        ))
        .expect("mapping");
        assert_eq!(
            (
                wire.work_items.len(),
                wire.attention_signals.len(),
                wire.reminders.len()
            ),
            (3, 6, 2)
        );
        let value = serde_json::to_value(&wire.inbox).expect("json");
        let kinds = value
            .as_array()
            .expect("inbox")
            .iter()
            .map(|entry| entry["kind"].as_str().expect("kind"))
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                "work_item",
                "attention_signal",
                "attention_signal",
                "attention_signal",
                "reminder_fire"
            ]
        );
    }

    #[test]
    fn historical_affected_views_are_frozen_after_live_roots_change() {
        let mut work = work_item(k::WorkItemLifecycle::Open);
        let mut signal = signal(
            k::SignalSourceLifecycle::Active,
            k::SignalAttentionState::Unread,
        );
        let mut reminder = reminder(&[k::ReminderFireState::Scheduled]);
        let fire_id = reminder.fires()[0].id();
        let historical = committed(
            k::ChangeKind::WorkItemCreated,
            vec![
                k::AffectedView::WorkItem {
                    work_item: work.clone(),
                },
                k::AffectedView::AttentionSignal {
                    attention_signal: signal.clone(),
                },
                k::AffectedView::Reminder {
                    reminder: reminder.clone(),
                },
            ],
            k::InboxEffects::default(),
        );
        work.complete().expect("complete");
        signal.acknowledge().expect("acknowledge");
        reminder.mark_fired(fire_id).expect("fire");
        let value = serde_json::to_value(event(&historical).expect("mapping")).expect("json");
        assert_eq!(value["affected"][0]["work_item"]["lifecycle"], "open");
        assert_eq!(
            value["affected"][1]["attention_signal"]["attention_state"],
            "unread"
        );
        assert_eq!(
            value["affected"][2]["reminder"]["fires"][0]["state"],
            "scheduled"
        );
    }

    #[test]
    fn source_order_modes_and_optional_ingress_links_map() {
        for (native, expected) in [
            (k::SourceOrderMode::Unordered, json!({"mode":"unordered"})),
            (ordered(None), json!({"mode":"ordered","domain":"sequence"})),
        ] {
            assert_eq!(
                serde_json::to_value(order(&native).expect("order")).expect("json"),
                expected
            );
        }
        let receipt = k::SourceReceipt::reconstruct(
            k::SourceReceiptId::new(),
            occurrence_key(),
            None,
            k::CanonicalFingerprint::reconstruct([0; 32]),
            k::SourceOrderMode::Unordered,
            at(10),
            at(11),
        )
        .expect("receipt");
        let value: Value =
            serde_json::to_value(super::receipt(&receipt).expect("mapping")).expect("json");
        assert!(value["source_entity_key"].is_null());
    }
}
