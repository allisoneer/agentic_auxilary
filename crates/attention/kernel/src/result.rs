//! Operation-specific semantic outcomes.

use crate::AttentionSignalId;
use crate::ChangeEventId;
use crate::CommitCursor;
use crate::OutboxIntentId;
use crate::ReminderFireId;
use crate::ReminderId;
use crate::SourceReceiptId;
use crate::WorkItemId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandDisposition {
    Applied,
    Replayed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome<T> {
    disposition: CommandDisposition,
    value: T,
    cursor: CommitCursor,
    change_event_id: ChangeEventId,
    outbox_intent_id: Option<OutboxIntentId>,
}

impl<T> CommandOutcome<T> {
    pub const fn new(
        disposition: CommandDisposition,
        value: T,
        cursor: CommitCursor,
        change_event_id: ChangeEventId,
        outbox_intent_id: Option<OutboxIntentId>,
    ) -> Self {
        Self {
            disposition,
            value,
            cursor,
            change_event_id,
            outbox_intent_id,
        }
    }

    pub const fn disposition(&self) -> CommandDisposition {
        self.disposition
    }

    pub const fn value(&self) -> &T {
        &self.value
    }

    pub const fn cursor(&self) -> CommitCursor {
        self.cursor
    }

    pub const fn change_event_id(&self) -> ChangeEventId {
        self.change_event_id
    }

    pub const fn outbox_intent_id(&self) -> Option<OutboxIntentId> {
        self.outbox_intent_id
    }
}

impl<T: Clone> CommandOutcome<T> {
    #[must_use]
    pub fn replayed(&self) -> Self {
        Self {
            disposition: CommandDisposition::Replayed,
            value: self.value.clone(),
            cursor: self.cursor,
            change_event_id: self.change_event_id,
            outbox_intent_id: self.outbox_intent_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkItemValue {
    id: WorkItemId,
}

impl WorkItemValue {
    pub const fn new(id: WorkItemId) -> Self {
        Self { id }
    }

    pub const fn id(self) -> WorkItemId {
        self.id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttentionSignalValue {
    id: AttentionSignalId,
}

impl AttentionSignalValue {
    pub const fn new(id: AttentionSignalId) -> Self {
        Self { id }
    }

    pub const fn id(self) -> AttentionSignalId {
        self.id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReminderValue {
    reminder_id: ReminderId,
    fire_id: ReminderFireId,
}

impl ReminderValue {
    pub const fn new(reminder_id: ReminderId, fire_id: ReminderFireId) -> Self {
        Self {
            reminder_id,
            fire_id,
        }
    }

    pub const fn reminder_id(self) -> ReminderId {
        self.reminder_id
    }

    pub const fn fire_id(self) -> ReminderFireId {
        self.fire_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptOnlyReason {
    Equal,
    Older,
    MissingOrderedValue,
    ComparatorDomainMismatch,
    Incomparable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceIngestionDecision {
    Advanced,
    ReceiptOnly(ReceiptOnlyReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceIngestionValue {
    receipt_id: SourceReceiptId,
    signal_id: AttentionSignalId,
    decision: SourceIngestionDecision,
}

impl SourceIngestionValue {
    pub const fn new(
        receipt_id: SourceReceiptId,
        signal_id: AttentionSignalId,
        decision: SourceIngestionDecision,
    ) -> Self {
        Self {
            receipt_id,
            signal_id,
            decision,
        }
    }

    pub const fn receipt_id(self) -> SourceReceiptId {
        self.receipt_id
    }

    pub const fn signal_id(self) -> AttentionSignalId {
        self.signal_id
    }

    pub const fn decision(self) -> SourceIngestionDecision {
        self.decision
    }
}

pub type CreateWorkItemResult = CommandOutcome<WorkItemValue>;
pub type CompleteWorkItemResult = CommandOutcome<WorkItemValue>;
pub type CancelWorkItemResult = CommandOutcome<WorkItemValue>;
pub type AcknowledgeAttentionSignalResult = CommandOutcome<AttentionSignalValue>;
pub type IngestSourceOccurrenceResult = CommandOutcome<SourceIngestionValue>;
pub type CreateReminderResult = CommandOutcome<ReminderValue>;
pub type FireReminderResult = CommandOutcome<ReminderValue>;
pub type AcknowledgeReminderFireResult = CommandOutcome<ReminderValue>;
pub type SnoozeReminderFireResult = CommandOutcome<ReminderValue>;
