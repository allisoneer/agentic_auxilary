//! Bounded semantic query inputs and results.

use crate::AcknowledgeAttentionSignalResult;
use crate::AcknowledgeReminderFireResult;
use crate::AttentionSnapshot;
use crate::BoundedDeliveryText;
use crate::CancelWorkItemResult;
use crate::CanonicalFingerprint;
use crate::ChangesResult;
use crate::ClaimLimit;
use crate::CommitCursor;
use crate::CompleteWorkItemResult;
use crate::CreateReminderResult;
use crate::CreateWorkItemResult;
use crate::DeliveryState;
use crate::FireReminderResult;
use crate::IngestSourceOccurrenceResult;
use crate::MutationIdempotencyKey;
use crate::MutationOperation;
use crate::OutboxIntent;
use crate::OutboxIntentId;
use crate::QueryLimit;
use crate::ReminderFireId;
use crate::ReminderId;
use crate::Revision;
use crate::SnoozeReminderFireResult;
use crate::SourceEntityKey;
use chrono::DateTime;
use chrono::Utc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PriorMutationOutcome {
    CreateWorkItem(CreateWorkItemResult),
    CompleteWorkItem(CompleteWorkItemResult),
    CancelWorkItem(CancelWorkItemResult),
    AcknowledgeAttentionSignal(AcknowledgeAttentionSignalResult),
    IngestSourceOccurrence(IngestSourceOccurrenceResult),
    CreateReminder(CreateReminderResult),
    FireReminder(FireReminderResult),
    AcknowledgeReminderFire(AcknowledgeReminderFireResult),
    SnoozeReminderFire(SnoozeReminderFireResult),
}

impl PriorMutationOutcome {
    #[must_use]
    pub fn replayed(&self) -> Self {
        match self {
            Self::CreateWorkItem(outcome) => Self::CreateWorkItem(outcome.replayed()),
            Self::CompleteWorkItem(outcome) => Self::CompleteWorkItem(outcome.replayed()),
            Self::CancelWorkItem(outcome) => Self::CancelWorkItem(outcome.replayed()),
            Self::AcknowledgeAttentionSignal(outcome) => {
                Self::AcknowledgeAttentionSignal(outcome.replayed())
            }
            Self::IngestSourceOccurrence(outcome) => {
                Self::IngestSourceOccurrence(outcome.replayed())
            }
            Self::CreateReminder(outcome) => Self::CreateReminder(outcome.replayed()),
            Self::FireReminder(outcome) => Self::FireReminder(outcome.replayed()),
            Self::AcknowledgeReminderFire(outcome) => {
                Self::AcknowledgeReminderFire(outcome.replayed())
            }
            Self::SnoozeReminderFire(outcome) => Self::SnoozeReminderFire(outcome.replayed()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorMutationRecord {
    operation: MutationOperation,
    fingerprint: CanonicalFingerprint,
    outcome: PriorMutationOutcome,
}

impl PriorMutationRecord {
    pub const fn new(
        operation: MutationOperation,
        fingerprint: CanonicalFingerprint,
        outcome: PriorMutationOutcome,
    ) -> Self {
        Self {
            operation,
            fingerprint,
            outcome,
        }
    }

    pub const fn operation(&self) -> MutationOperation {
        self.operation
    }

    pub const fn fingerprint(&self) -> CanonicalFingerprint {
        self.fingerprint
    }

    pub const fn outcome(&self) -> &PriorMutationOutcome {
        &self.outcome
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriorOutcomeQuery {
    key: MutationIdempotencyKey,
}

impl PriorOutcomeQuery {
    pub const fn new(key: MutationIdempotencyKey) -> Self {
        Self { key }
    }

    pub const fn key(self) -> MutationIdempotencyKey {
        self.key
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangesAfterQuery {
    after: CommitCursor,
    limit: QueryLimit,
}

impl ChangesAfterQuery {
    pub const fn new(after: CommitCursor, limit: QueryLimit) -> Self {
        Self { after, limit }
    }

    pub const fn after(self) -> CommitCursor {
        self.after
    }

    pub const fn limit(self) -> QueryLimit {
        self.limit
    }
}

pub type SnapshotResult = AttentionSnapshot;
pub type ChangesAfterResult = ChangesResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueReminderFiresQuery {
    due_at_or_before: DateTime<Utc>,
    limit: QueryLimit,
}

impl DueReminderFiresQuery {
    pub const fn new(due_at_or_before: DateTime<Utc>, limit: QueryLimit) -> Self {
        Self {
            due_at_or_before,
            limit,
        }
    }

    pub const fn due_at_or_before(&self) -> &DateTime<Utc> {
        &self.due_at_or_before
    }

    pub const fn limit(self) -> QueryLimit {
        self.limit
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueReminderFire {
    reminder_id: ReminderId,
    fire_id: ReminderFireId,
    reminder_revision: Revision,
    trigger_at: DateTime<Utc>,
}

impl DueReminderFire {
    pub const fn new(
        reminder_id: ReminderId,
        fire_id: ReminderFireId,
        reminder_revision: Revision,
        trigger_at: DateTime<Utc>,
    ) -> Self {
        Self {
            reminder_id,
            fire_id,
            reminder_revision,
            trigger_at,
        }
    }

    pub const fn reminder_id(self) -> ReminderId {
        self.reminder_id
    }

    pub const fn fire_id(self) -> ReminderFireId {
        self.fire_id
    }

    pub const fn reminder_revision(self) -> Revision {
        self.reminder_revision
    }

    pub const fn trigger_at(&self) -> &DateTime<Utc> {
        &self.trigger_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryAuthority {
    intent: OutboxIntent,
    state: DeliveryState,
}

impl DeliveryAuthority {
    pub const fn new(intent: OutboxIntent, state: DeliveryState) -> Self {
        Self { intent, state }
    }

    pub const fn intent(&self) -> &OutboxIntent {
        &self.intent
    }

    pub const fn state(&self) -> &DeliveryState {
        &self.state
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeliveryClaimQuery {
    eligible_at: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
    limit: ClaimLimit,
}

impl DeliveryClaimQuery {
    pub const fn new(
        eligible_at: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
        limit: ClaimLimit,
    ) -> Self {
        Self {
            eligible_at,
            lease_expires_at,
            limit,
        }
    }

    pub const fn eligible_at(&self) -> &DateTime<Utc> {
        &self.eligible_at
    }

    pub const fn lease_expires_at(&self) -> &DateTime<Utc> {
        &self.lease_expires_at
    }

    pub const fn limit(self) -> ClaimLimit {
        self.limit
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointQuery {
    worker: BoundedDeliveryText,
}

impl CheckpointQuery {
    pub const fn new(worker: BoundedDeliveryText) -> Self {
        Self { worker }
    }

    pub const fn worker(&self) -> &BoundedDeliveryText {
        &self.worker
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointAdvance {
    worker: BoundedDeliveryText,
    expected: Option<CommitCursor>,
    next: CommitCursor,
    terminal_intent_id: OutboxIntentId,
}

impl CheckpointAdvance {
    pub const fn new(
        worker: BoundedDeliveryText,
        expected: Option<CommitCursor>,
        next: CommitCursor,
        terminal_intent_id: OutboxIntentId,
    ) -> Self {
        Self {
            worker,
            expected,
            next,
            terminal_intent_id,
        }
    }

    pub const fn worker(&self) -> &BoundedDeliveryText {
        &self.worker
    }

    pub const fn expected(&self) -> Option<CommitCursor> {
        self.expected
    }

    pub const fn next(&self) -> CommitCursor {
        self.next
    }

    pub const fn terminal_intent_id(&self) -> OutboxIntentId {
        self.terminal_intent_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceAuthorityQuery {
    key: SourceEntityKey,
}

impl SourceAuthorityQuery {
    pub const fn new(key: SourceEntityKey) -> Self {
        Self { key }
    }

    pub const fn key(&self) -> &SourceEntityKey {
        &self.key
    }
}
