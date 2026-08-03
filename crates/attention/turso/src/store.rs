use crate::AttentionDatabase;
use crate::Error;
use attention_kernel::AcknowledgeAttentionSignalBundle;
use attention_kernel::AcknowledgeAttentionSignalResult;
use attention_kernel::AcknowledgeReminderFireBundle;
use attention_kernel::AcknowledgeReminderFireResult;
use attention_kernel::AttentionCommitPort;
use attention_kernel::AttentionReadPort;
use attention_kernel::AttentionSignal;
use attention_kernel::AttentionSignalId;
use attention_kernel::CancelWorkItemBundle;
use attention_kernel::CancelWorkItemResult;
use attention_kernel::ChangesAfterQuery;
use attention_kernel::ChangesAfterResult;
use attention_kernel::CompleteWorkItemBundle;
use attention_kernel::CompleteWorkItemResult;
use attention_kernel::CreateReminderBundle;
use attention_kernel::CreateReminderResult;
use attention_kernel::CreateWorkItemBundle;
use attention_kernel::CreateWorkItemResult;
use attention_kernel::FireReminderBundle;
use attention_kernel::FireReminderResult;
use attention_kernel::IngestSourceOccurrenceBundle;
use attention_kernel::IngestSourceOccurrenceResult;
use attention_kernel::PortError;
use attention_kernel::PriorMutationOutcome;
use attention_kernel::PriorOutcomeQuery;
use attention_kernel::Reminder;
use attention_kernel::ReminderId;
use attention_kernel::SnapshotResult;
use attention_kernel::SnoozeReminderFireBundle;
use attention_kernel::SnoozeReminderFireResult;
use attention_kernel::SourceAuthorityQuery;
use attention_kernel::SourceEntity;
use attention_kernel::SourceReceipt;
use attention_kernel::SourceReceiptId;
use attention_kernel::WorkItem;
use attention_kernel::WorkItemId;
use futures::future::BoxFuture;

impl AttentionReadPort for AttentionDatabase {
    type Error = Error;

    fn work_item(
        &self,
        id: WorkItemId,
    ) -> BoxFuture<'_, Result<Option<WorkItem>, PortError<Self::Error>>> {
        Box::pin(async move {
            self.semantic_work_item(id)
                .await
                .map_err(PortError::Adapter)
        })
    }

    fn attention_signal(
        &self,
        id: AttentionSignalId,
    ) -> BoxFuture<'_, Result<Option<AttentionSignal>, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_signal(id).await.map_err(PortError::Adapter) })
    }

    fn reminder(
        &self,
        id: ReminderId,
    ) -> BoxFuture<'_, Result<Option<Reminder>, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_reminder(id).await.map_err(PortError::Adapter) })
    }

    fn source_entity(
        &self,
        query: SourceAuthorityQuery,
    ) -> BoxFuture<'_, Result<Option<SourceEntity>, PortError<Self::Error>>> {
        Box::pin(async move {
            self.semantic_entity(&query)
                .await
                .map_err(PortError::Adapter)
        })
    }

    fn source_receipt(
        &self,
        id: SourceReceiptId,
    ) -> BoxFuture<'_, Result<Option<SourceReceipt>, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_receipt(id).await.map_err(PortError::Adapter) })
    }

    fn prior_outcome(
        &self,
        query: PriorOutcomeQuery,
    ) -> BoxFuture<'_, Result<Option<PriorMutationOutcome>, PortError<Self::Error>>> {
        Box::pin(async move {
            self.semantic_outcome(query.key())
                .await
                .map_err(PortError::Adapter)
        })
    }

    fn snapshot(&self) -> BoxFuture<'_, Result<SnapshotResult, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_snapshot().await.map_err(PortError::Adapter) })
    }

    fn changes_after(
        &self,
        query: ChangesAfterQuery,
    ) -> BoxFuture<'_, Result<ChangesAfterResult, PortError<Self::Error>>> {
        Box::pin(async move {
            self.semantic_changes(query)
                .await
                .map_err(PortError::Adapter)
        })
    }
}

impl AttentionCommitPort for AttentionDatabase {
    type Error = Error;

    fn commit_create_work_item(
        &self,
        bundle: CreateWorkItemBundle,
    ) -> BoxFuture<'_, Result<CreateWorkItemResult, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_create_work_item(bundle).await })
    }
    fn commit_complete_work_item(
        &self,
        bundle: CompleteWorkItemBundle,
    ) -> BoxFuture<'_, Result<CompleteWorkItemResult, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_complete_work_item(bundle).await })
    }
    fn commit_cancel_work_item(
        &self,
        bundle: CancelWorkItemBundle,
    ) -> BoxFuture<'_, Result<CancelWorkItemResult, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_cancel_work_item(bundle).await })
    }
    fn commit_acknowledge_attention_signal(
        &self,
        bundle: AcknowledgeAttentionSignalBundle,
    ) -> BoxFuture<'_, Result<AcknowledgeAttentionSignalResult, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_acknowledge_signal(bundle).await })
    }
    fn commit_ingest_source_occurrence(
        &self,
        bundle: IngestSourceOccurrenceBundle,
    ) -> BoxFuture<'_, Result<IngestSourceOccurrenceResult, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_ingest_source(bundle).await })
    }
    fn commit_create_reminder(
        &self,
        bundle: CreateReminderBundle,
    ) -> BoxFuture<'_, Result<CreateReminderResult, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_create_reminder(bundle).await })
    }
    fn commit_fire_reminder(
        &self,
        bundle: FireReminderBundle,
    ) -> BoxFuture<'_, Result<FireReminderResult, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_fire_reminder(bundle).await })
    }
    fn commit_acknowledge_reminder_fire(
        &self,
        bundle: AcknowledgeReminderFireBundle,
    ) -> BoxFuture<'_, Result<AcknowledgeReminderFireResult, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_acknowledge_reminder(bundle).await })
    }
    fn commit_snooze_reminder_fire(
        &self,
        bundle: SnoozeReminderFireBundle,
    ) -> BoxFuture<'_, Result<SnoozeReminderFireResult, PortError<Self::Error>>> {
        Box::pin(async move { self.semantic_snooze_reminder(bundle).await })
    }
}
