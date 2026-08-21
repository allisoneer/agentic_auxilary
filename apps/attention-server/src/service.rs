use crate::mapping;
use attention_kernel as kernel;
use attention_protocol as protocol;
use futures_util::future::BoxFuture;
use std::error::Error;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid request parameters")]
    InvalidParams,
    #[error("semantic operation failure: {0}")]
    Semantic(#[from] kernel::SemanticError),
    #[error("service adapter failed")]
    Adapter,
}

pub fn peer_error(error: &ServiceError) -> Option<protocol::RpcError> {
    let ServiceError::Semantic(error) = error else {
        return None;
    };
    let resource = |resource: &kernel::ResourceRef| match resource {
        kernel::ResourceRef::WorkItem(id) => protocol::ResourceRef::WorkItem {
            id: mapping::work_item_id(*id),
        },
        kernel::ResourceRef::AttentionSignal(id) => protocol::ResourceRef::AttentionSignal {
            id: mapping::signal_id(*id),
        },
        kernel::ResourceRef::Reminder(id) => protocol::ResourceRef::Reminder {
            id: mapping::reminder_id(*id),
        },
        kernel::ResourceRef::ReminderFire(id) => protocol::ResourceRef::ReminderFire {
            id: mapping::fire_id(*id),
        },
        kernel::ResourceRef::SourceEntity(key) => protocol::ResourceRef::SourceEntity {
            key: mapping::source_key(key),
        },
        kernel::ResourceRef::SourceOccurrence(key) => protocol::ResourceRef::SourceOccurrence {
            key: mapping::occurrence(key),
        },
    };
    let typed = match error {
        kernel::SemanticError::NotFound(value) => {
            protocol::V1Error::ResourceNotFound(protocol::ResourceNotFoundData {
                resource: resource(value),
            })
        }
        kernel::SemanticError::ExpectedRevisionConflict {
            resource: value,
            expected,
            actual,
        } => protocol::V1Error::ExpectedRevisionConflict(protocol::ExpectedRevisionConflictData {
            resource: resource(value),
            expected: mapping::revision(*expected).ok()?,
            actual: mapping::revision(*actual).ok()?,
        }),
        kernel::SemanticError::CreateConflict(value) => {
            protocol::V1Error::CreateConflict(protocol::CreateConflictData {
                resource: resource(value),
            })
        }
        kernel::SemanticError::IdempotencyMismatch(value) => {
            protocol::V1Error::IdempotencyMismatch(protocol::IdempotencyMismatchData {
                idempotency_key: protocol::MutationIdempotencyKey(value.to_string()),
            })
        }
        kernel::SemanticError::OccurrenceContentMismatch(value) => {
            protocol::V1Error::OccurrenceContentMismatch(protocol::OccurrenceContentMismatchData {
                occurrence_key: mapping::occurrence(value),
            })
        }
        kernel::SemanticError::ObservedSourceVersionConflict {
            entity,
            observed,
            actual,
        } => protocol::V1Error::SourceVersionConflict(protocol::SourceVersionConflictData {
            entity: mapping::source_key(entity),
            observed: observed.map(mapping::version).transpose().ok()?,
            actual: actual.map(mapping::version).transpose().ok()?,
        }),
    };
    protocol::RpcError::try_from(typed).ok()
}

pub trait AttentionService: Send + Sync {
    fn work_item(
        &self,
        id: kernel::WorkItemId,
    ) -> BoxFuture<'_, Result<Option<kernel::WorkItem>, ServiceError>>;
    fn attention_signal(
        &self,
        id: kernel::AttentionSignalId,
    ) -> BoxFuture<'_, Result<Option<kernel::AttentionSignal>, ServiceError>>;
    fn reminder(
        &self,
        id: kernel::ReminderId,
    ) -> BoxFuture<'_, Result<Option<kernel::Reminder>, ServiceError>>;
    fn source_entity(
        &self,
        query: kernel::SourceAuthorityQuery,
    ) -> BoxFuture<'_, Result<Option<kernel::SourceEntity>, ServiceError>>;
    fn source_receipt(
        &self,
        id: kernel::SourceReceiptId,
    ) -> BoxFuture<'_, Result<Option<kernel::SourceReceipt>, ServiceError>>;
    fn snapshot(&self) -> BoxFuture<'_, Result<kernel::AttentionSnapshot, ServiceError>>;
    fn changes_after(
        &self,
        query: kernel::ChangesAfterQuery,
    ) -> BoxFuture<'_, Result<kernel::ChangesResult, ServiceError>>;
}

pub struct ReadPortService<P>(pub P);

impl<P> AttentionService for ReadPortService<P>
where
    P: kernel::AttentionReadPort,
    P::Error: Error + Send + Sync + 'static,
{
    fn work_item(
        &self,
        id: kernel::WorkItemId,
    ) -> BoxFuture<'_, Result<Option<kernel::WorkItem>, ServiceError>> {
        Box::pin(async move { self.0.work_item(id).await.map_err(map_port) })
    }
    fn attention_signal(
        &self,
        id: kernel::AttentionSignalId,
    ) -> BoxFuture<'_, Result<Option<kernel::AttentionSignal>, ServiceError>> {
        Box::pin(async move { self.0.attention_signal(id).await.map_err(map_port) })
    }
    fn reminder(
        &self,
        id: kernel::ReminderId,
    ) -> BoxFuture<'_, Result<Option<kernel::Reminder>, ServiceError>> {
        Box::pin(async move { self.0.reminder(id).await.map_err(map_port) })
    }
    fn source_entity(
        &self,
        query: kernel::SourceAuthorityQuery,
    ) -> BoxFuture<'_, Result<Option<kernel::SourceEntity>, ServiceError>> {
        Box::pin(async move { self.0.source_entity(query).await.map_err(map_port) })
    }
    fn source_receipt(
        &self,
        id: kernel::SourceReceiptId,
    ) -> BoxFuture<'_, Result<Option<kernel::SourceReceipt>, ServiceError>> {
        Box::pin(async move { self.0.source_receipt(id).await.map_err(map_port) })
    }
    fn snapshot(&self) -> BoxFuture<'_, Result<kernel::AttentionSnapshot, ServiceError>> {
        Box::pin(async move { self.0.snapshot().await.map_err(map_port) })
    }
    fn changes_after(
        &self,
        query: kernel::ChangesAfterQuery,
    ) -> BoxFuture<'_, Result<kernel::ChangesResult, ServiceError>> {
        Box::pin(async move { self.0.changes_after(query).await.map_err(map_port) })
    }
}

fn map_port<E>(error: kernel::PortError<E>) -> ServiceError {
    match error {
        kernel::PortError::Semantic(error) => ServiceError::Semantic(error),
        kernel::PortError::Adapter(_) => ServiceError::Adapter,
    }
}

pub trait AttentionMutationService: Send + Sync {
    fn create_work_item(
        &self,
        command: kernel::CreateWorkItem,
    ) -> BoxFuture<'_, Result<kernel::CreateWorkItemResult, ServiceError>>;
    fn complete_work_item(
        &self,
        command: kernel::CompleteWorkItem,
    ) -> BoxFuture<'_, Result<kernel::CompleteWorkItemResult, ServiceError>>;
    fn cancel_work_item(
        &self,
        command: kernel::CancelWorkItem,
    ) -> BoxFuture<'_, Result<kernel::CancelWorkItemResult, ServiceError>>;
    fn acknowledge_attention_signal(
        &self,
        command: kernel::AcknowledgeAttentionSignal,
    ) -> BoxFuture<'_, Result<kernel::AcknowledgeAttentionSignalResult, ServiceError>>;
    fn ingest_source_occurrence(
        &self,
        command: kernel::IngestSourceOccurrence,
    ) -> BoxFuture<'_, Result<kernel::IngestSourceOccurrenceResult, ServiceError>>;
    fn create_reminder(
        &self,
        command: kernel::CreateReminder,
    ) -> BoxFuture<'_, Result<kernel::CreateReminderResult, ServiceError>>;
    fn acknowledge_reminder_fire(
        &self,
        command: kernel::AcknowledgeReminderFire,
    ) -> BoxFuture<'_, Result<kernel::AcknowledgeReminderFireResult, ServiceError>>;
    fn snooze_reminder_fire(
        &self,
        command: kernel::SnoozeReminderFire,
    ) -> BoxFuture<'_, Result<kernel::SnoozeReminderFireResult, ServiceError>>;
}

pub trait DeliveryWorkerService: Send + Sync {
    fn claim(
        &self,
        query: kernel::DeliveryClaimQuery,
    ) -> BoxFuture<'_, Result<Vec<kernel::DeliveryClaim>, ServiceError>>;
    fn inspect(
        &self,
        intent_id: kernel::OutboxIntentId,
    ) -> BoxFuture<'_, Result<Option<kernel::DeliveryAuthority>, ServiceError>>;
    fn renew(
        &self,
        intent_id: kernel::OutboxIntentId,
        token: kernel::DeliveryLeaseToken,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> BoxFuture<'_, Result<kernel::RenewOutcome, ServiceError>>;
    fn succeed(
        &self,
        intent_id: kernel::OutboxIntentId,
        token: kernel::DeliveryLeaseToken,
        provider_message_id: kernel::ProviderMessageId,
        succeeded_at: chrono::DateTime<chrono::Utc>,
    ) -> BoxFuture<'_, Result<kernel::DeliveryCompletionOutcome, ServiceError>>;
    fn fail_retryable(
        &self,
        intent_id: kernel::OutboxIntentId,
        token: kernel::DeliveryLeaseToken,
        attempt: u32,
        error: kernel::BoundedDeliveryText,
        next_retry_at: chrono::DateTime<chrono::Utc>,
    ) -> BoxFuture<'_, Result<kernel::DeliveryCompletionOutcome, ServiceError>>;
    fn fail_terminal(
        &self,
        intent_id: kernel::OutboxIntentId,
        token: kernel::DeliveryLeaseToken,
        attempt: u32,
        error: kernel::BoundedDeliveryText,
        failed_at: chrono::DateTime<chrono::Utc>,
    ) -> BoxFuture<'_, Result<kernel::DeliveryCompletionOutcome, ServiceError>>;
}

pub struct DeliveryPortService<P>(pub P);

impl<P> DeliveryWorkerService for DeliveryPortService<P>
where
    P: kernel::DeliveryPort,
    P::Error: Error + Send + Sync + 'static,
{
    fn claim(
        &self,
        query: kernel::DeliveryClaimQuery,
    ) -> BoxFuture<'_, Result<Vec<kernel::DeliveryClaim>, ServiceError>> {
        Box::pin(async move { self.0.claim(query).await.map_err(map_port) })
    }
    fn inspect(
        &self,
        intent_id: kernel::OutboxIntentId,
    ) -> BoxFuture<'_, Result<Option<kernel::DeliveryAuthority>, ServiceError>> {
        Box::pin(async move { self.0.inspect(intent_id).await.map_err(map_port) })
    }
    fn renew(
        &self,
        intent_id: kernel::OutboxIntentId,
        token: kernel::DeliveryLeaseToken,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> BoxFuture<'_, Result<kernel::RenewOutcome, ServiceError>> {
        Box::pin(async move {
            self.0
                .renew(intent_id, token, expires_at)
                .await
                .map_err(map_port)
        })
    }
    fn succeed(
        &self,
        intent_id: kernel::OutboxIntentId,
        token: kernel::DeliveryLeaseToken,
        provider_message_id: kernel::ProviderMessageId,
        succeeded_at: chrono::DateTime<chrono::Utc>,
    ) -> BoxFuture<'_, Result<kernel::DeliveryCompletionOutcome, ServiceError>> {
        Box::pin(async move {
            self.0
                .succeed(intent_id, token, provider_message_id, succeeded_at)
                .await
                .map_err(map_port)
        })
    }
    fn fail_retryable(
        &self,
        intent_id: kernel::OutboxIntentId,
        token: kernel::DeliveryLeaseToken,
        attempt: u32,
        error: kernel::BoundedDeliveryText,
        next_retry_at: chrono::DateTime<chrono::Utc>,
    ) -> BoxFuture<'_, Result<kernel::DeliveryCompletionOutcome, ServiceError>> {
        Box::pin(async move {
            self.0
                .fail_retryable(intent_id, token, attempt, error, next_retry_at)
                .await
                .map_err(map_port)
        })
    }
    fn fail_terminal(
        &self,
        intent_id: kernel::OutboxIntentId,
        token: kernel::DeliveryLeaseToken,
        attempt: u32,
        error: kernel::BoundedDeliveryText,
        failed_at: chrono::DateTime<chrono::Utc>,
    ) -> BoxFuture<'_, Result<kernel::DeliveryCompletionOutcome, ServiceError>> {
        Box::pin(async move {
            self.0
                .fail_terminal(intent_id, token, attempt, error, failed_at)
                .await
                .map_err(map_port)
        })
    }
}

pub type SharedDeliveryWorkerService = Arc<dyn DeliveryWorkerService>;
pub type SharedMutationService = Arc<dyn AttentionMutationService>;
pub type SharedService = Arc<dyn AttentionService>;

#[cfg(test)]
mod tests {
    use super::*;

    fn source_key() -> kernel::SourceEntityKey {
        kernel::SourceEntityKey::new(
            kernel::SourceKind::new("linear").expect("kind"),
            kernel::SourceInstance::new("primary").expect("instance"),
            kernel::ExternalEntityId::new("ENG-1110").expect("external id"),
        )
    }
    fn occurrence() -> kernel::OccurrenceKey {
        kernel::OccurrenceKey::new(
            kernel::SourceKind::new("linear").expect("kind"),
            kernel::SourceInstance::new("primary").expect("instance"),
            kernel::OccurrenceId::new("delivery-1").expect("occurrence"),
        )
    }
    fn assert_code(error: kernel::SemanticError, code: protocol::ErrorCode) {
        let mapped = peer_error(&ServiceError::Semantic(error)).expect("peer error");
        assert_eq!(mapped.code, code);
        assert!(mapped.data.is_some());
        protocol::V1Error::try_from(mapped).expect("canonical typed error");
    }

    #[test]
    fn semantic_error_mapping_covers_every_resource_and_variant() {
        let resources = [
            kernel::ResourceRef::WorkItem(kernel::WorkItemId::new()),
            kernel::ResourceRef::AttentionSignal(kernel::AttentionSignalId::new()),
            kernel::ResourceRef::Reminder(kernel::ReminderId::new()),
            kernel::ResourceRef::ReminderFire(kernel::ReminderFireId::new()),
            kernel::ResourceRef::SourceEntity(source_key()),
            kernel::ResourceRef::SourceOccurrence(occurrence()),
        ];
        for resource in resources.clone() {
            assert_code(
                kernel::SemanticError::NotFound(resource),
                protocol::RESOURCE_NOT_FOUND,
            );
        }
        for resource in resources.clone() {
            assert_code(
                kernel::SemanticError::CreateConflict(resource),
                protocol::CREATE_CONFLICT,
            );
        }
        for resource in resources {
            assert_code(
                kernel::SemanticError::ExpectedRevisionConflict {
                    resource,
                    expected: kernel::Revision::initial(),
                    actual: kernel::Revision::try_from(2).expect("revision"),
                },
                protocol::EXPECTED_REVISION_CONFLICT,
            );
        }
        assert_code(
            kernel::SemanticError::IdempotencyMismatch(kernel::MutationIdempotencyKey::new()),
            protocol::IDEMPOTENCY_MISMATCH,
        );
        assert_code(
            kernel::SemanticError::OccurrenceContentMismatch(occurrence()),
            protocol::OCCURRENCE_CONTENT_MISMATCH,
        );
        for (observed, actual) in [
            (None, None),
            (Some(kernel::SourceStateVersion::initial()), None),
            (None, Some(kernel::SourceStateVersion::initial())),
            (
                Some(kernel::SourceStateVersion::initial()),
                Some(kernel::SourceStateVersion::try_from(2).expect("version")),
            ),
        ] {
            assert_code(
                kernel::SemanticError::ObservedSourceVersionConflict {
                    entity: source_key(),
                    observed,
                    actual,
                },
                protocol::SOURCE_VERSION_CONFLICT,
            );
        }
        assert!(peer_error(&ServiceError::Adapter).is_none());
    }
}
