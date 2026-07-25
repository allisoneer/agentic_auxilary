//! Public mutation parameters and cursor-bearing semantic outcomes.

use crate::AttentionSignalId;
use crate::ChangeEventId;
use crate::Cursor;
use crate::MutationIdempotencyKey;
use crate::OccurrenceKey;
use crate::OutboxIntentId;
use crate::ReminderFireId;
use crate::ReminderId;
use crate::ReminderTarget;
use crate::Revision;
use crate::SignalSourceLifecycle;
use crate::SourceEntityId;
use crate::SourceEntityKey;
use crate::SourceOrder;
use crate::SourceReceiptId;
use crate::WireTimestamp;
use crate::WorkItemId;
use serde::Deserialize;
use serde::Serialize;

/// Whether a successful mutation was newly applied or replayed idempotently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationDisposition {
    Applied,
    Replayed,
}

/// Successful mutation identity and commit position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationResult<V> {
    pub disposition: MutationDisposition,
    pub value: V,
    pub cursor: Cursor,
    pub change_event_id: ChangeEventId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outbox_intent_id: Option<OutboxIntentId>,
}

/// Value returned by `WorkItem` mutations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemMutationValue {
    pub id: WorkItemId,
}

/// Value returned by `AttentionSignal` mutations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionSignalMutationValue {
    pub id: AttentionSignalId,
}

/// Value returned by `Reminder` and `ReminderFire` mutations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderMutationValue {
    pub reminder_id: ReminderId,
    pub fire_id: ReminderFireId,
}

/// Receipt-only reason when source authority did not advance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptOnlyReason {
    Equal,
    Older,
    MissingOrderedValue,
    ComparatorDomainMismatch,
    Incomparable,
}

/// Source-ingress authority decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum SourceIngestionDecision {
    Advanced,
    ReceiptOnly { reason: ReceiptOnlyReason },
}

/// Value returned by source occurrence ingestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIngestionValue {
    pub receipt_id: SourceReceiptId,
    pub signal_id: AttentionSignalId,
    pub decision: SourceIngestionDecision,
}

pub type CreateWorkItemResult = MutationResult<WorkItemMutationValue>;
pub type CompleteWorkItemResult = MutationResult<WorkItemMutationValue>;
pub type CancelWorkItemResult = MutationResult<WorkItemMutationValue>;
pub type AcknowledgeAttentionSignalResult = MutationResult<AttentionSignalMutationValue>;
pub type IngestSourceOccurrenceResult = MutationResult<SourceIngestionValue>;
pub type CreateReminderResult = MutationResult<ReminderMutationValue>;
pub type AcknowledgeReminderFireResult = MutationResult<ReminderMutationValue>;
pub type SnoozeReminderFireResult = MutationResult<ReminderMutationValue>;

/// Parameters for creating a `WorkItem`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorkItemParams {
    pub id: WorkItemId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_at: Option<WireTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<WireTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_until: Option<WireTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_link: Option<SourceEntityKey>,
    pub idempotency_key: MutationIdempotencyKey,
}

macro_rules! existing_root_params {
    ($(#[$meta:meta])* $name:ident, $id_name:ident, $id_type:ty) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name {
            pub $id_name: $id_type,
            pub expected_revision: Revision,
            pub idempotency_key: MutationIdempotencyKey,
        }
    };
}

existing_root_params!(
    /// Parameters for completing a `WorkItem`.
    CompleteWorkItemParams,
    id,
    WorkItemId
);
existing_root_params!(
    /// Parameters for cancelling a `WorkItem`.
    CancelWorkItemParams,
    id,
    WorkItemId
);
existing_root_params!(
    /// Parameters for acknowledging an `AttentionSignal`.
    AcknowledgeAttentionSignalParams,
    id,
    AttentionSignalId
);

/// Optional source-entity identity supplied during occurrence ingestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEntityIdentity {
    pub id: SourceEntityId,
    pub key: SourceEntityKey,
}

/// Parameters for ingesting one source occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestSourceOccurrenceParams {
    pub receipt_id: SourceReceiptId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<SourceEntityIdentity>,
    pub signal_id: AttentionSignalId,
    pub occurrence_key: OccurrenceKey,
    pub occurred_at: WireTimestamp,
    pub order: SourceOrder,
    pub source_lifecycle: SignalSourceLifecycle,
    pub fresh_attention: bool,
    pub idempotency_key: MutationIdempotencyKey,
}

/// Parameters for creating a Reminder and its initial fire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateReminderParams {
    pub reminder_id: ReminderId,
    pub initial_fire_id: ReminderFireId,
    pub target: ReminderTarget,
    pub trigger_at: WireTimestamp,
    pub idempotency_key: MutationIdempotencyKey,
}

/// Parameters for acknowledging a fired `ReminderFire`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcknowledgeReminderFireParams {
    pub reminder_id: ReminderId,
    pub fire_id: ReminderFireId,
    pub expected_revision: Revision,
    pub idempotency_key: MutationIdempotencyKey,
}

/// Parameters for snoozing a fired `ReminderFire`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnoozeReminderFireParams {
    pub reminder_id: ReminderId,
    pub fire_id: ReminderFireId,
    pub replacement_fire_id: ReminderFireId,
    pub replacement_trigger_at: WireTimestamp,
    pub expected_revision: Revision,
    pub idempotency_key: MutationIdempotencyKey,
}
