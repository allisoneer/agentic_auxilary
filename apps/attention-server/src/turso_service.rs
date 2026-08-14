use crate::publication::PublicationHub;
use crate::service::AttentionMutationService;
use crate::service::AttentionService;
use crate::service::ServiceError;
use attention_kernel as k;
use attention_turso::AttentionDatabase;
use chrono::Timelike;
use chrono::Utc;
use futures_util::future::BoxFuture;
use tokio::sync::Mutex;

#[cfg(feature = "test-support")]
static FAIL_AFTER_COMMIT_BEFORE_PUBLICATION: std::sync::Mutex<Option<String>> =
    std::sync::Mutex::new(None);

/// Arms an integration-test failure for one exact mutation after commit and before publication.
#[cfg(feature = "test-support")]
pub fn fail_after_commit_before_publication_once(key: attention_protocol::MutationIdempotencyKey) {
    *FAIL_AFTER_COMMIT_BEFORE_PUBLICATION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(key.0);
}

#[cfg(feature = "test-support")]
fn fail_after_commit(key: k::MutationIdempotencyKey) {
    let armed = FAIL_AFTER_COMMIT_BEFORE_PUBLICATION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take_if(|armed| armed == &key.to_string())
        .is_some();
    assert!(
        !armed,
        "injected post-commit pre-publication connection loss"
    );
}

/// Production semantic adapter. The gate covers preflight, evaluation, commit,
/// frozen-event lookup, and publication so subscribers observe commit order.
pub struct TursoAttentionService {
    database: AttentionDatabase,
    publications: PublicationHub,
    mutation_gate: Mutex<()>,
}

impl TursoAttentionService {
    pub fn new(database: AttentionDatabase, publications: PublicationHub) -> Self {
        Self {
            database,
            publications,
            mutation_gate: Mutex::new(()),
        }
    }

    fn context_at(outbox: bool, evaluated_at: chrono::DateTime<Utc>) -> k::EvaluationContext {
        k::EvaluationContext::new(
            k::ChangeEventId::new(),
            outbox.then(k::OutboxIntentId::new),
            evaluated_at,
        )
    }

    fn now() -> chrono::DateTime<Utc> {
        let now = Utc::now();
        now.with_nanosecond((now.timestamp_subsec_nanos() / 1_000) * 1_000)
            .unwrap_or(now)
    }

    fn context(outbox: bool) -> k::EvaluationContext {
        Self::context_at(outbox, Self::now())
    }

    async fn prior<C: k::CanonicalCommand + Sync>(
        &self,
        command: &C,
    ) -> Result<Option<k::PriorMutationOutcome>, ServiceError> {
        let record = k::MutationReplayPort::prior_mutation(
            &self.database,
            k::PriorOutcomeQuery::new(command.idempotency_key()),
        )
        .await
        .map_err(map_port)?;
        let Some(record) = record else {
            return Ok(None);
        };
        let same_operation = record.operation() == command.operation();
        let same_fingerprint = record.fingerprint() == command.canonical_fingerprint();
        if !same_operation || !same_fingerprint {
            return Err(k::SemanticError::IdempotencyMismatch(command.idempotency_key()).into());
        }
        Ok(Some(record.outcome().replayed()))
    }

    async fn publish<T>(
        &self,
        outcome: k::CommandOutcome<T>,
    ) -> Result<k::CommandOutcome<T>, ServiceError> {
        let event = k::ChangeEventReadPort::change_event(&self.database, outcome.change_event_id())
            .await
            .map_err(map_port)?
            .ok_or(ServiceError::Adapter)?;
        if event.cursor() != outcome.cursor() {
            return Err(ServiceError::Adapter);
        }
        self.publications.publish(&event);
        Ok(outcome)
    }

    /// Fire one scheduler snapshot. Every candidate is re-read and evaluated while holding the
    /// same gate used by public mutations; commit, exact frozen-event lookup, and publication are
    /// consequently ordered as one server-side critical section.
    pub(crate) async fn fire_due(
        &self,
        now: chrono::DateTime<Utc>,
        batch_size: usize,
        shutdown: &tokio_util::sync::CancellationToken,
    ) -> Result<(), ServiceError> {
        let limit = k::QueryLimit::try_from(batch_size).map_err(|_| ServiceError::InvalidParams)?;
        let due = k::ReminderSchedulePort::due_reminder_fires(
            &self.database,
            k::DueReminderFiresQuery::new(now, limit),
        )
        .await
        .map_err(map_port)?;
        for candidate in due {
            if shutdown.is_cancelled() {
                break;
            }
            let _gate = self.mutation_gate.lock().await;
            let Some(current) =
                k::AttentionReadPort::reminder(&self.database, candidate.reminder_id())
                    .await
                    .map_err(map_port)?
            else {
                continue;
            };
            let still_due = current.revision() == candidate.reminder_revision()
                && current.fires().iter().any(|fire| {
                    fire.id() == candidate.fire_id()
                        && fire.state() == k::ReminderFireState::Scheduled
                        && fire.trigger_at() <= &now
                });
            if !still_due {
                continue;
            }
            let command = k::FireReminder::new(
                candidate.reminder_id(),
                candidate.fire_id(),
                k::MutationIdempotencyKey::new(),
            );
            let bundle = k::evaluate_fire_reminder(&command, &current, Self::context_at(true, now))
                .map_err(map_evaluation)?;
            let result = k::AttentionCommitPort::commit_fire_reminder(&self.database, bundle)
                .await
                .map_err(map_port)?;
            self.publish(result).await?;
        }
        Ok(())
    }
}

fn map_port<E>(error: k::PortError<E>) -> ServiceError {
    match error {
        k::PortError::Semantic(error) => error.into(),
        k::PortError::Adapter(_) => ServiceError::Adapter,
    }
}
fn map_evaluation(error: k::EvaluationError) -> ServiceError {
    match error {
        k::EvaluationError::Semantic(error) => error.into(),
        k::EvaluationError::Invariant(_) => ServiceError::InvalidParams,
    }
}

impl AttentionService for TursoAttentionService {
    fn work_item(
        &self,
        id: k::WorkItemId,
    ) -> BoxFuture<'_, Result<Option<k::WorkItem>, ServiceError>> {
        Box::pin(async move {
            k::AttentionReadPort::work_item(&self.database, id)
                .await
                .map_err(map_port)
        })
    }
    fn attention_signal(
        &self,
        id: k::AttentionSignalId,
    ) -> BoxFuture<'_, Result<Option<k::AttentionSignal>, ServiceError>> {
        Box::pin(async move {
            k::AttentionReadPort::attention_signal(&self.database, id)
                .await
                .map_err(map_port)
        })
    }
    fn reminder(
        &self,
        id: k::ReminderId,
    ) -> BoxFuture<'_, Result<Option<k::Reminder>, ServiceError>> {
        Box::pin(async move {
            k::AttentionReadPort::reminder(&self.database, id)
                .await
                .map_err(map_port)
        })
    }
    fn source_entity(
        &self,
        query: k::SourceAuthorityQuery,
    ) -> BoxFuture<'_, Result<Option<k::SourceEntity>, ServiceError>> {
        Box::pin(async move {
            k::AttentionReadPort::source_entity(&self.database, query)
                .await
                .map_err(map_port)
        })
    }
    fn source_receipt(
        &self,
        id: k::SourceReceiptId,
    ) -> BoxFuture<'_, Result<Option<k::SourceReceipt>, ServiceError>> {
        Box::pin(async move {
            k::AttentionReadPort::source_receipt(&self.database, id)
                .await
                .map_err(map_port)
        })
    }
    fn snapshot(&self) -> BoxFuture<'_, Result<k::AttentionSnapshot, ServiceError>> {
        Box::pin(async move {
            k::AttentionReadPort::snapshot(&self.database)
                .await
                .map_err(map_port)
        })
    }
    fn changes_after(
        &self,
        query: k::ChangesAfterQuery,
    ) -> BoxFuture<'_, Result<k::ChangesResult, ServiceError>> {
        Box::pin(async move {
            k::AttentionReadPort::changes_after(&self.database, query)
                .await
                .map_err(map_port)
        })
    }
}

impl AttentionMutationService for TursoAttentionService {
    fn create_work_item(
        &self,
        command: k::CreateWorkItem,
    ) -> BoxFuture<'_, Result<k::CreateWorkItemResult, ServiceError>> {
        Box::pin(async move {
            let _gate = self.mutation_gate.lock().await;
            if let Some(prior) = self.prior(&command).await? {
                if let k::PriorMutationOutcome::CreateWorkItem(value) = prior {
                    return self.publish(value).await;
                }
                return Err(ServiceError::Adapter);
            }
            let bundle = k::evaluate_create_work_item(&command, Self::context(false));
            let result = k::AttentionCommitPort::commit_create_work_item(&self.database, bundle)
                .await
                .map_err(map_port)?;
            #[cfg(feature = "test-support")]
            fail_after_commit(command.idempotency_key());
            self.publish(result).await
        })
    }
    fn complete_work_item(
        &self,
        command: k::CompleteWorkItem,
    ) -> BoxFuture<'_, Result<k::CompleteWorkItemResult, ServiceError>> {
        Box::pin(async move {
            let _gate = self.mutation_gate.lock().await;
            if let Some(prior) = self.prior(&command).await? {
                if let k::PriorMutationOutcome::CompleteWorkItem(value) = prior {
                    return self.publish(value).await;
                }
                return Err(ServiceError::Adapter);
            }
            let current = k::AttentionReadPort::work_item(&self.database, command.id())
                .await
                .map_err(map_port)?
                .ok_or_else(|| {
                    k::SemanticError::NotFound(k::ResourceRef::WorkItem(command.id()))
                })?;
            let bundle = k::evaluate_complete_work_item(&command, &current, Self::context(false))
                .map_err(map_evaluation)?;
            let result = k::AttentionCommitPort::commit_complete_work_item(&self.database, bundle)
                .await
                .map_err(map_port)?;
            self.publish(result).await
        })
    }
    fn cancel_work_item(
        &self,
        command: k::CancelWorkItem,
    ) -> BoxFuture<'_, Result<k::CancelWorkItemResult, ServiceError>> {
        Box::pin(async move {
            let _gate = self.mutation_gate.lock().await;
            if let Some(prior) = self.prior(&command).await? {
                if let k::PriorMutationOutcome::CancelWorkItem(value) = prior {
                    return self.publish(value).await;
                }
                return Err(ServiceError::Adapter);
            }
            let current = k::AttentionReadPort::work_item(&self.database, command.id())
                .await
                .map_err(map_port)?
                .ok_or_else(|| {
                    k::SemanticError::NotFound(k::ResourceRef::WorkItem(command.id()))
                })?;
            let bundle = k::evaluate_cancel_work_item(&command, &current, Self::context(false))
                .map_err(map_evaluation)?;
            let result = k::AttentionCommitPort::commit_cancel_work_item(&self.database, bundle)
                .await
                .map_err(map_port)?;
            self.publish(result).await
        })
    }
    fn acknowledge_attention_signal(
        &self,
        command: k::AcknowledgeAttentionSignal,
    ) -> BoxFuture<'_, Result<k::AcknowledgeAttentionSignalResult, ServiceError>> {
        Box::pin(async move {
            let _gate = self.mutation_gate.lock().await;
            if let Some(prior) = self.prior(&command).await? {
                if let k::PriorMutationOutcome::AcknowledgeAttentionSignal(value) = prior {
                    return self.publish(value).await;
                }
                return Err(ServiceError::Adapter);
            }
            let current = k::AttentionReadPort::attention_signal(&self.database, command.id())
                .await
                .map_err(map_port)?
                .ok_or_else(|| {
                    k::SemanticError::NotFound(k::ResourceRef::AttentionSignal(command.id()))
                })?;
            let bundle =
                k::evaluate_acknowledge_attention_signal(&command, &current, Self::context(false))
                    .map_err(map_evaluation)?;
            let result =
                k::AttentionCommitPort::commit_acknowledge_attention_signal(&self.database, bundle)
                    .await
                    .map_err(map_port)?;
            self.publish(result).await
        })
    }
    fn ingest_source_occurrence(
        &self,
        command: k::IngestSourceOccurrence,
    ) -> BoxFuture<'_, Result<k::IngestSourceOccurrenceResult, ServiceError>> {
        Box::pin(async move {
            let _gate = self.mutation_gate.lock().await;
            if let Some(prior) = self.prior(&command).await? {
                if let k::PriorMutationOutcome::IngestSourceOccurrence(value) = prior {
                    return self.publish(value).await;
                }
                return Err(ServiceError::Adapter);
            }
            let entity = if let Some(identity) = command.entity() {
                k::AttentionReadPort::source_entity(
                    &self.database,
                    k::SourceAuthorityQuery::new(identity.key().clone()),
                )
                .await
                .map_err(map_port)?
            } else {
                None
            };
            let signal =
                k::AttentionReadPort::attention_signal(&self.database, command.signal_id())
                    .await
                    .map_err(map_port)?;
            let bundle = k::evaluate_ingest_source_occurrence(
                &command,
                entity.as_ref(),
                signal.as_ref(),
                Self::context_at(command.fresh_attention(), *command.ingested_at()),
            )
            .map_err(map_evaluation)?;
            let result =
                k::AttentionCommitPort::commit_ingest_source_occurrence(&self.database, bundle)
                    .await
                    .map_err(map_port)?;
            self.publish(result).await
        })
    }
    fn create_reminder(
        &self,
        command: k::CreateReminder,
    ) -> BoxFuture<'_, Result<k::CreateReminderResult, ServiceError>> {
        Box::pin(async move {
            let _gate = self.mutation_gate.lock().await;
            if let Some(prior) = self.prior(&command).await? {
                if let k::PriorMutationOutcome::CreateReminder(value) = prior {
                    return self.publish(value).await;
                }
                return Err(ServiceError::Adapter);
            }
            let bundle = k::evaluate_create_reminder(&command, Self::context(false));
            let result = k::AttentionCommitPort::commit_create_reminder(&self.database, bundle)
                .await
                .map_err(map_port)?;
            self.publish(result).await
        })
    }
    fn acknowledge_reminder_fire(
        &self,
        command: k::AcknowledgeReminderFire,
    ) -> BoxFuture<'_, Result<k::AcknowledgeReminderFireResult, ServiceError>> {
        Box::pin(async move {
            let _gate = self.mutation_gate.lock().await;
            if let Some(prior) = self.prior(&command).await? {
                if let k::PriorMutationOutcome::AcknowledgeReminderFire(value) = prior {
                    return self.publish(value).await;
                }
                return Err(ServiceError::Adapter);
            }
            let current = k::AttentionReadPort::reminder(&self.database, command.reminder_id())
                .await
                .map_err(map_port)?
                .ok_or_else(|| {
                    k::SemanticError::NotFound(k::ResourceRef::Reminder(command.reminder_id()))
                })?;
            let bundle =
                k::evaluate_acknowledge_reminder_fire(&command, &current, Self::context(false))
                    .map_err(map_evaluation)?;
            let result =
                k::AttentionCommitPort::commit_acknowledge_reminder_fire(&self.database, bundle)
                    .await
                    .map_err(map_port)?;
            self.publish(result).await
        })
    }
    fn snooze_reminder_fire(
        &self,
        command: k::SnoozeReminderFire,
    ) -> BoxFuture<'_, Result<k::SnoozeReminderFireResult, ServiceError>> {
        Box::pin(async move {
            let _gate = self.mutation_gate.lock().await;
            if let Some(prior) = self.prior(&command).await? {
                if let k::PriorMutationOutcome::SnoozeReminderFire(value) = prior {
                    return self.publish(value).await;
                }
                return Err(ServiceError::Adapter);
            }
            let current = k::AttentionReadPort::reminder(&self.database, command.reminder_id())
                .await
                .map_err(map_port)?
                .ok_or_else(|| {
                    k::SemanticError::NotFound(k::ResourceRef::Reminder(command.reminder_id()))
                })?;
            let bundle = k::evaluate_snooze_reminder_fire(&command, &current, Self::context(false))
                .map_err(map_evaluation)?;
            let result =
                k::AttentionCommitPort::commit_snooze_reminder_fire(&self.database, bundle)
                    .await
                    .map_err(map_port)?;
            self.publish(result).await
        })
    }
}
