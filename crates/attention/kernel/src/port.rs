//! Object-safe asynchronous semantic ports.
//!
//! Commit implementations must resolve replay and every guard in the same transaction as all
//! bundle effects. Failed guards write nothing. Change publication occurs only after commit.
//! Source-version conflicts are retryable by re-reading and re-evaluating in kernel code.
//! Delivery sends occur outside these transactions; fresh claim tokens fence stale workers.

use crate::AcknowledgeAttentionSignalBundle;
use crate::AcknowledgeAttentionSignalResult;
use crate::AcknowledgeReminderFireBundle;
use crate::AcknowledgeReminderFireResult;
use crate::AttentionSignal;
use crate::AttentionSignalId;
use crate::BoundedDeliveryText;
use crate::CancelWorkItemBundle;
use crate::CancelWorkItemResult;
use crate::ChangesAfterQuery;
use crate::ChangesAfterResult;
use crate::CheckpointAdvance;
use crate::CheckpointQuery;
use crate::CompleteWorkItemBundle;
use crate::CompleteWorkItemResult;
use crate::CreateReminderBundle;
use crate::CreateReminderResult;
use crate::CreateWorkItemBundle;
use crate::CreateWorkItemResult;
use crate::DeliveryAuthority;
use crate::DeliveryCheckpoint;
use crate::DeliveryClaim;
use crate::DeliveryClaimQuery;
use crate::DeliveryCompletionOutcome;
use crate::DeliveryLeaseToken;
use crate::DueReminderFire;
use crate::DueReminderFiresQuery;
use crate::FireReminderBundle;
use crate::FireReminderResult;
use crate::IngestSourceOccurrenceBundle;
use crate::IngestSourceOccurrenceResult;
use crate::OutboxIntentId;
use crate::PortError;
use crate::PriorMutationOutcome;
use crate::PriorOutcomeQuery;
use crate::ProviderMessageId;
use crate::Reminder;
use crate::ReminderId;
use crate::RenewOutcome;
use crate::SnapshotResult;
use crate::SnoozeReminderFireBundle;
use crate::SnoozeReminderFireResult;
use crate::SourceAuthorityQuery;
use crate::SourceEntity;
use crate::WorkItem;
use crate::WorkItemId;
use chrono::DateTime;
use chrono::Utc;
use futures::future::BoxFuture;
use std::error::Error;

pub trait AttentionReadPort: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    fn work_item(
        &self,
        id: WorkItemId,
    ) -> BoxFuture<'_, Result<Option<WorkItem>, PortError<Self::Error>>>;

    fn attention_signal(
        &self,
        id: AttentionSignalId,
    ) -> BoxFuture<'_, Result<Option<AttentionSignal>, PortError<Self::Error>>>;

    fn reminder(
        &self,
        id: ReminderId,
    ) -> BoxFuture<'_, Result<Option<Reminder>, PortError<Self::Error>>>;

    fn source_entity(
        &self,
        query: SourceAuthorityQuery,
    ) -> BoxFuture<'_, Result<Option<SourceEntity>, PortError<Self::Error>>>;

    fn prior_outcome(
        &self,
        query: PriorOutcomeQuery,
    ) -> BoxFuture<'_, Result<Option<PriorMutationOutcome>, PortError<Self::Error>>>;

    fn snapshot(&self) -> BoxFuture<'_, Result<SnapshotResult, PortError<Self::Error>>>;

    fn changes_after(
        &self,
        query: ChangesAfterQuery,
    ) -> BoxFuture<'_, Result<ChangesAfterResult, PortError<Self::Error>>>;
}

pub trait AttentionCommitPort: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    fn commit_create_work_item(
        &self,
        bundle: CreateWorkItemBundle,
    ) -> BoxFuture<'_, Result<CreateWorkItemResult, PortError<Self::Error>>>;

    fn commit_complete_work_item(
        &self,
        bundle: CompleteWorkItemBundle,
    ) -> BoxFuture<'_, Result<CompleteWorkItemResult, PortError<Self::Error>>>;

    fn commit_cancel_work_item(
        &self,
        bundle: CancelWorkItemBundle,
    ) -> BoxFuture<'_, Result<CancelWorkItemResult, PortError<Self::Error>>>;

    fn commit_acknowledge_attention_signal(
        &self,
        bundle: AcknowledgeAttentionSignalBundle,
    ) -> BoxFuture<'_, Result<AcknowledgeAttentionSignalResult, PortError<Self::Error>>>;

    fn commit_ingest_source_occurrence(
        &self,
        bundle: IngestSourceOccurrenceBundle,
    ) -> BoxFuture<'_, Result<IngestSourceOccurrenceResult, PortError<Self::Error>>>;

    fn commit_create_reminder(
        &self,
        bundle: CreateReminderBundle,
    ) -> BoxFuture<'_, Result<CreateReminderResult, PortError<Self::Error>>>;

    fn commit_fire_reminder(
        &self,
        bundle: FireReminderBundle,
    ) -> BoxFuture<'_, Result<FireReminderResult, PortError<Self::Error>>>;

    fn commit_acknowledge_reminder_fire(
        &self,
        bundle: AcknowledgeReminderFireBundle,
    ) -> BoxFuture<'_, Result<AcknowledgeReminderFireResult, PortError<Self::Error>>>;

    fn commit_snooze_reminder_fire(
        &self,
        bundle: SnoozeReminderFireBundle,
    ) -> BoxFuture<'_, Result<SnoozeReminderFireResult, PortError<Self::Error>>>;
}

pub trait ReminderSchedulePort: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    fn due_reminder_fires(
        &self,
        query: DueReminderFiresQuery,
    ) -> BoxFuture<'_, Result<Vec<DueReminderFire>, PortError<Self::Error>>>;
}

pub trait DeliveryPort: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    fn claim(
        &self,
        query: DeliveryClaimQuery,
    ) -> BoxFuture<'_, Result<Vec<DeliveryClaim>, PortError<Self::Error>>>;

    fn inspect(
        &self,
        intent_id: OutboxIntentId,
    ) -> BoxFuture<'_, Result<Option<DeliveryAuthority>, PortError<Self::Error>>>;

    fn renew(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        expires_at: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<RenewOutcome, PortError<Self::Error>>>;

    fn succeed(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        provider_message_id: ProviderMessageId,
        succeeded_at: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<DeliveryCompletionOutcome, PortError<Self::Error>>>;

    fn fail_retryable(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        attempt: u32,
        error: BoundedDeliveryText,
        next_retry_at: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<DeliveryCompletionOutcome, PortError<Self::Error>>>;

    fn fail_terminal(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        attempt: u32,
        error: BoundedDeliveryText,
        failed_at: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<DeliveryCompletionOutcome, PortError<Self::Error>>>;

    fn skip(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        reason: BoundedDeliveryText,
        skipped_at: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<DeliveryCompletionOutcome, PortError<Self::Error>>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointAdvanceOutcome {
    Advanced,
    Repeated,
    Conflict,
    TerminalStateRequired,
}

pub trait DeliveryCheckpointPort: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    fn checkpoint(
        &self,
        query: CheckpointQuery,
    ) -> BoxFuture<'_, Result<Option<DeliveryCheckpoint>, PortError<Self::Error>>>;

    fn advance_checkpoint(
        &self,
        advance: CheckpointAdvance,
    ) -> BoxFuture<'_, Result<CheckpointAdvanceOutcome, PortError<Self::Error>>>;
}

#[cfg(test)]
mod tests {
    use super::AttentionCommitPort;
    use super::AttentionReadPort;
    use super::DeliveryCheckpointPort;
    use super::DeliveryPort;
    use super::ReminderSchedulePort;
    use std::convert::Infallible;

    #[test]
    fn every_port_is_object_safe() {
        fn read(_: &dyn AttentionReadPort<Error = Infallible>) {}
        fn commit(_: &dyn AttentionCommitPort<Error = Infallible>) {}
        fn schedule(_: &dyn ReminderSchedulePort<Error = Infallible>) {}
        fn delivery(_: &dyn DeliveryPort<Error = Infallible>) {}
        fn checkpoint(_: &dyn DeliveryCheckpointPort<Error = Infallible>) {}

        let _ = (read, commit, schedule, delivery, checkpoint);
    }
}
