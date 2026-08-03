use crate::Error;
use crate::codec;
use crate::decode;
use crate::domain_sql;
use crate::mapping;
use crate::reader::ReaderPool;
use crate::semantic_reader;
use crate::writer::TransactionDecision;
use crate::writer::Writer;
use attention_kernel::AcknowledgeAttentionSignalBundle;
use attention_kernel::AcknowledgeAttentionSignalResult;
use attention_kernel::AcknowledgeReminderFireBundle;
use attention_kernel::AcknowledgeReminderFireResult;
use attention_kernel::AtomicEffects;
use attention_kernel::CancelWorkItemBundle;
use attention_kernel::CancelWorkItemResult;
use attention_kernel::CommandDisposition;
use attention_kernel::CommandOutcome;
use attention_kernel::CommitCursor;
use attention_kernel::CompleteWorkItemBundle;
use attention_kernel::CompleteWorkItemResult;
use attention_kernel::CreateReminderBundle;
use attention_kernel::CreateReminderResult;
use attention_kernel::CreateWorkItemBundle;
use attention_kernel::CreateWorkItemResult;
use attention_kernel::ExpectedRevisionGuard;
use attention_kernel::FireReminderBundle;
use attention_kernel::FireReminderResult;
use attention_kernel::IdempotencyCommit;
use attention_kernel::IngestSourceOccurrenceBundle;
use attention_kernel::IngestSourceOccurrenceResult;
use attention_kernel::ObservedSourceAuthority;
use attention_kernel::OutboxIntent;
use attention_kernel::PortError;
use attention_kernel::PriorMutationOutcome;
use attention_kernel::Reminder;
use attention_kernel::ReminderFireId;
use attention_kernel::ReminderFireState;
use attention_kernel::ReminderMutationGuards;
use attention_kernel::ReminderTarget;
use attention_kernel::ResourceRef;
use attention_kernel::SemanticError;
use attention_kernel::SnoozeReminderFireBundle;
use attention_kernel::SnoozeReminderFireResult;
use attention_kernel::WorkItem;
use futures::FutureExt;
use tokio::sync::Mutex;
use turso_db::Database;
use turso_db::params;
use turso_db::transaction::Transaction;

enum Begin {
    Fresh(CommitCursor),
    Replay(PriorMutationOutcome),
}

fn semantic<T>(error: SemanticError) -> Result<T, PortError<Error>> {
    Err(PortError::Semantic(error))
}

async fn stream_head(transaction: &Transaction<'_>) -> Result<CommitCursor, PortError<Error>> {
    let mut rows = transaction
        .query(domain_sql::SELECT_STREAM_STATE, ())
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    let row = rows
        .next()
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?
        .ok_or_else(|| PortError::Adapter(Error::MigrationIntegrity("stream state is missing")))?;
    let head = CommitCursor::try_from(
        mapping::parse_counter(&decode::blob(&row, 0).map_err(PortError::Adapter)?)
            .map_err(PortError::Adapter)?,
    )
    .map_err(|error| PortError::Adapter(Error::Decode(Box::new(error))))?;
    drop(rows);
    Ok(head)
}

fn next_cursor(head: CommitCursor) -> Result<CommitCursor, PortError<Error>> {
    CommitCursor::try_from(
        head.value()
            .checked_add(1)
            .ok_or_else(|| PortError::Adapter(Error::MigrationIntegrity("cursor overflow")))?,
    )
    .map_err(|error| PortError::Adapter(Error::Decode(Box::new(error))))
}

async fn begin(
    transaction: &Transaction<'_>,
    idempotency: &IdempotencyCommit,
    outcome: &PriorMutationOutcome,
) -> Result<Begin, PortError<Error>> {
    let head = stream_head(transaction).await?;
    let cursor = next_cursor(head)?;
    let (version, bytes) = codec::encode_outcome(outcome).map_err(PortError::Adapter)?;
    let affected = transaction
        .execute(
            domain_sql::INSERT_OUTCOME,
            params![
                mapping::id(idempotency.key()),
                mapping::operation(idempotency.operation()),
                mapping::fingerprint(idempotency.fingerprint()),
                version,
                bytes
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    if affected != 0 {
        return Ok(Begin::Fresh(cursor));
    }
    let mut rows = transaction
        .query(
            domain_sql::SELECT_OUTCOME,
            params![mapping::id(idempotency.key())],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    let row = rows
        .next()
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?
        .ok_or_else(|| {
            PortError::Adapter(Error::MigrationIntegrity("outcome conflict vanished"))
        })?;
    let operation = mapping::parse_operation(decode::integer(&row, 0).map_err(PortError::Adapter)?)
        .map_err(PortError::Adapter)?;
    let fingerprint =
        mapping::parse_fingerprint(&decode::blob(&row, 1).map_err(PortError::Adapter)?)
            .map_err(PortError::Adapter)?;
    let stored = codec::decode_outcome_for_operation(
        operation,
        decode::integer(&row, 2).map_err(PortError::Adapter)?,
        &decode::blob(&row, 3).map_err(PortError::Adapter)?,
    )
    .map_err(PortError::Adapter)?;
    drop(rows);
    if operation != idempotency.operation() || fingerprint != idempotency.fingerprint() {
        return semantic(SemanticError::IdempotencyMismatch(idempotency.key()));
    }
    Ok(Begin::Replay(stored.replayed()))
}

fn applied<T: Clone>(
    value: &T,
    effects: &AtomicEffects,
    cursor: CommitCursor,
) -> CommandOutcome<T> {
    CommandOutcome::new(
        CommandDisposition::Applied,
        value.clone(),
        cursor,
        effects.change().id(),
        effects.outbox_intent().map(OutboxIntent::id),
    )
}

async fn finish(
    transaction: &Transaction<'_>,
    prior_head: CommitCursor,
    cursor: CommitCursor,
    effects: &AtomicEffects,
) -> Result<(), PortError<Error>> {
    let (version, bytes) = codec::encode_event(effects.change()).map_err(PortError::Adapter)?;
    transaction
        .execute(
            domain_sql::INSERT_EVENT,
            params![
                mapping::counter(cursor.value()),
                mapping::id(effects.change().id()),
                mapping::timestamp(effects.change().occurred_at()),
                mapping::change_kind(effects.change().kind()),
                version,
                bytes
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    if let Some(intent) = effects.outbox_intent() {
        let (subject_kind, subject_id) = match intent.subject() {
            attention_kernel::DeliverySubject::AttentionSignal(id) => (0, mapping::id(id)),
            attention_kernel::DeliverySubject::ReminderFire(id) => (1, mapping::id(id)),
        };
        let purpose = match intent.purpose() {
            attention_kernel::DeliveryPurpose::FreshAttention => 0,
            attention_kernel::DeliveryPurpose::ReminderFired => 1,
        };
        transaction
            .execute(
                domain_sql::INSERT_OUTBOX,
                params![
                    mapping::id(intent.id()),
                    intent.deduplication_key().as_str(),
                    subject_kind,
                    subject_id,
                    mapping::id(intent.originating_change_event_id()),
                    mapping::timestamp(intent.created_at()),
                    purpose
                ],
            )
            .await
            .map_err(Error::from)
            .map_err(PortError::Adapter)?;
    }
    let affected = transaction
        .execute(
            domain_sql::UPDATE_STREAM_HEAD,
            params![
                mapping::counter(cursor.value()),
                mapping::counter(prior_head.value())
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    if affected != 1 {
        return Err(PortError::Adapter(Error::MigrationIntegrity(
            "stream head update lost arbitration",
        )));
    }
    Ok(())
}

fn work_item_columns(item: &WorkItem) -> (Option<String>, Option<String>, Option<String>) {
    item.source_link().map_or((None, None, None), |key| {
        (
            Some(key.source_kind().as_str().to_string()),
            Some(key.source_instance().as_str().to_string()),
            Some(key.external_entity_id().as_str().to_string()),
        )
    })
}

async fn insert_work_item(
    transaction: &Transaction<'_>,
    item: &WorkItem,
) -> Result<u64, PortError<Error>> {
    let (kind, instance, external) = work_item_columns(item);
    transaction
        .execute(
            domain_sql::INSERT_WORK_ITEM,
            params![
                mapping::id(item.id()),
                mapping::counter(item.revision().value()),
                mapping::work_item_lifecycle(item.lifecycle()),
                item.due_at().map(mapping::timestamp),
                item.scheduled_at().map(mapping::timestamp),
                item.defer_until().map(mapping::timestamp),
                kind,
                instance,
                external
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)
}

async fn update_work_item(
    transaction: &Transaction<'_>,
    item: &WorkItem,
    guard: &ExpectedRevisionGuard,
) -> Result<(), PortError<Error>> {
    let (kind, instance, external) = work_item_columns(item);
    let affected = transaction
        .execute(
            domain_sql::UPDATE_WORK_ITEM,
            params![
                mapping::id(item.id()),
                mapping::counter(item.revision().value()),
                mapping::work_item_lifecycle(item.lifecycle()),
                item.due_at().map(mapping::timestamp),
                item.scheduled_at().map(mapping::timestamp),
                item.defer_until().map(mapping::timestamp),
                kind,
                instance,
                external,
                mapping::counter(guard.expected().value())
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    if affected == 1 {
        return Ok(());
    }
    let mut rows = transaction
        .query(
            domain_sql::SELECT_WORK_ITEM,
            params![mapping::id(item.id())],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?
    else {
        return semantic(SemanticError::NotFound(ResourceRef::WorkItem(item.id())));
    };
    let actual = mapping::work_item(&row)
        .map_err(PortError::Adapter)?
        .revision();
    drop(rows);
    semantic(SemanticError::ExpectedRevisionConflict {
        resource: ResourceRef::WorkItem(item.id()),
        expected: guard.expected(),
        actual,
    })
}

async fn insert_eventual_signal(
    transaction: &Transaction<'_>,
    signal: &attention_kernel::AttentionSignal,
) -> Result<(), PortError<Error>> {
    transaction
        .execute(
            domain_sql::INSERT_SIGNAL,
            params![
                mapping::id(signal.id()),
                mapping::counter(signal.revision().value()),
                mapping::signal_source_lifecycle(signal.source_lifecycle()),
                mapping::signal_attention_state(signal.attention_state()),
                mapping::id(signal.source_receipt_id()),
                signal.source_entity_id().map(mapping::id)
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    Ok(())
}

async fn update_signal_guarded(
    transaction: &Transaction<'_>,
    signal: &attention_kernel::AttentionSignal,
    guard: &ExpectedRevisionGuard,
) -> Result<(), PortError<Error>> {
    let affected = transaction
        .execute(
            domain_sql::UPDATE_SIGNAL_GUARDED,
            params![
                mapping::id(signal.id()),
                mapping::counter(signal.revision().value()),
                mapping::signal_source_lifecycle(signal.source_lifecycle()),
                mapping::signal_attention_state(signal.attention_state()),
                mapping::id(signal.source_receipt_id()),
                signal.source_entity_id().map(mapping::id),
                mapping::counter(guard.expected().value())
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    if affected == 1 {
        return Ok(());
    }
    let mut rows = transaction
        .query(domain_sql::SELECT_SIGNAL, params![mapping::id(signal.id())])
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?
    else {
        return semantic(SemanticError::NotFound(ResourceRef::AttentionSignal(
            signal.id(),
        )));
    };
    let actual = mapping::signal(&row)
        .map_err(PortError::Adapter)?
        .revision();
    drop(rows);
    semantic(SemanticError::ExpectedRevisionConflict {
        resource: ResourceRef::AttentionSignal(signal.id()),
        expected: guard.expected(),
        actual,
    })
}

async fn resolve<T>(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    idempotency: &IdempotencyCommit,
    extract: impl FnOnce(PriorMutationOutcome) -> Option<T>,
) -> Result<T, PortError<Error>> {
    let stored = semantic_reader::stored_outcome(readers, engine, idempotency.key())
        .await
        .map_err(|_| PortError::Adapter(Error::CommitOutcomeUnknown))?
        .ok_or(PortError::Adapter(Error::CommitOutcomeUnknown))?;
    if stored.operation != idempotency.operation()
        || stored.fingerprint != idempotency.fingerprint()
    {
        return semantic(SemanticError::IdempotencyMismatch(idempotency.key()));
    }
    extract(stored.outcome.replayed()).ok_or(PortError::Adapter(Error::CommitOutcomeUnknown))
}

async fn resolve_source(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    idempotency: &IdempotencyCommit,
    occurrence: &attention_kernel::OccurrenceKey,
    occurrence_fingerprint: attention_kernel::CanonicalFingerprint,
) -> Result<IngestSourceOccurrenceResult, PortError<Error>> {
    match semantic_reader::stored_outcome(readers, engine, idempotency.key()).await {
        Ok(Some(stored)) => {
            if stored.operation != idempotency.operation()
                || stored.fingerprint != idempotency.fingerprint()
            {
                return semantic(SemanticError::IdempotencyMismatch(idempotency.key()));
            }
            let PriorMutationOutcome::IngestSourceOccurrence(outcome) = stored.outcome.replayed()
            else {
                return Err(PortError::Adapter(Error::CommitOutcomeUnknown));
            };
            Ok(outcome)
        }
        Ok(None) => {
            let stored = semantic_reader::stored_occurrence_outcome(readers, engine, occurrence)
                .await
                .map_err(|_| PortError::Adapter(Error::CommitOutcomeUnknown))?
                .ok_or(PortError::Adapter(Error::CommitOutcomeUnknown))?;
            if stored.fingerprint != occurrence_fingerprint {
                return semantic(SemanticError::OccurrenceContentMismatch(occurrence.clone()));
            }
            let PriorMutationOutcome::IngestSourceOccurrence(outcome) = stored.outcome.replayed()
            else {
                return Err(PortError::Adapter(Error::CommitOutcomeUnknown));
            };
            Ok(outcome)
        }
        Err(_) => Err(PortError::Adapter(Error::CommitOutcomeUnknown)),
    }
}

macro_rules! root_commit {
    ($function:ident, $bundle:ty, $result:ty, $prior:ident, $write:path) => {
        pub async fn $function(
            writer: &Writer,
            readers: &ReaderPool,
            engine: &Mutex<Option<Database>>,
            bundle: $bundle,
        ) -> Result<$result, PortError<Error>> {
            let identity = bundle.idempotency().clone();
            let resolution_identity = identity.clone();
            #[cfg(test)]
            let semantic_failpoint = writer.semantic_failpoint();
            let result = writer
                .with_immediate(engine, move |transaction| {
                    async move {
                        let head = stream_head(transaction).await?;
                        let cursor = next_cursor(head)?;
                        let result = applied(bundle.value(), bundle.effects(), cursor);
                        let prior = PriorMutationOutcome::$prior(result.clone());
                        match begin(transaction, bundle.idempotency(), &prior).await? {
                            Begin::Replay(PriorMutationOutcome::$prior(stored)) => {
                                return Ok(TransactionDecision::Rollback(stored));
                            }
                            Begin::Replay(_) => {
                                return Err(PortError::Adapter(Error::CommitOutcomeUnknown));
                            }
                            Begin::Fresh(actual) if actual == cursor => {}
                            Begin::Fresh(_) => {
                                return Err(PortError::Adapter(Error::CommitOutcomeUnknown));
                            }
                        }
                        #[cfg(test)]
                        crate::writer::semantic_fail(
                            semantic_failpoint,
                            crate::writer::SemanticFailpoint::AfterOutcome,
                        )?;
                        $write(transaction, &bundle).await?;
                        #[cfg(test)]
                        crate::writer::semantic_fail(
                            semantic_failpoint,
                            crate::writer::SemanticFailpoint::AfterRoot,
                        )?;
                        finish(transaction, head, cursor, bundle.effects()).await?;
                        #[cfg(test)]
                        crate::writer::semantic_fail(
                            semantic_failpoint,
                            crate::writer::SemanticFailpoint::AfterFinish,
                        )?;
                        Ok(TransactionDecision::Commit(result))
                    }
                    .boxed()
                })
                .await;
            match result {
                Err(PortError::Adapter(Error::CommitOutcomeUnknown)) => {
                    resolve(
                        readers,
                        engine,
                        &resolution_identity,
                        |outcome| match outcome {
                            PriorMutationOutcome::$prior(value) => Some(value),
                            _ => None,
                        },
                    )
                    .await
                }
                other => other,
            }
        }
    };
}

async fn create_work_item_root(
    transaction: &Transaction<'_>,
    bundle: &CreateWorkItemBundle,
) -> Result<(), PortError<Error>> {
    if insert_work_item(transaction, bundle.root()).await? == 0 {
        return semantic(SemanticError::CreateConflict(ResourceRef::WorkItem(
            bundle.root().id(),
        )));
    }
    Ok(())
}

async fn complete_work_item_root(
    transaction: &Transaction<'_>,
    bundle: &CompleteWorkItemBundle,
) -> Result<(), PortError<Error>> {
    update_work_item(transaction, bundle.root(), bundle.guard()).await
}

async fn cancel_work_item_root(
    transaction: &Transaction<'_>,
    bundle: &CancelWorkItemBundle,
) -> Result<(), PortError<Error>> {
    update_work_item(transaction, bundle.root(), bundle.guard()).await
}

async fn acknowledge_signal_root(
    transaction: &Transaction<'_>,
    bundle: &AcknowledgeAttentionSignalBundle,
) -> Result<(), PortError<Error>> {
    update_signal_guarded(transaction, bundle.root(), bundle.guard()).await
}

root_commit!(
    create_work_item,
    CreateWorkItemBundle,
    CreateWorkItemResult,
    CreateWorkItem,
    create_work_item_root
);

root_commit!(
    complete_work_item,
    CompleteWorkItemBundle,
    CompleteWorkItemResult,
    CompleteWorkItem,
    complete_work_item_root
);

root_commit!(
    cancel_work_item,
    CancelWorkItemBundle,
    CancelWorkItemResult,
    CancelWorkItem,
    cancel_work_item_root
);

root_commit!(
    acknowledge_signal,
    AcknowledgeAttentionSignalBundle,
    AcknowledgeAttentionSignalResult,
    AcknowledgeAttentionSignal,
    acknowledge_signal_root
);

async fn insert_receipt(
    transaction: &Transaction<'_>,
    bundle: &IngestSourceOccurrenceBundle,
) -> Result<u64, PortError<Error>> {
    let receipt = bundle.receipt();
    let key = receipt.occurrence_key();
    let (entity_kind, entity_instance, external) =
        receipt
            .source_entity_key()
            .map_or((None, None, None), |entity| {
                (
                    Some(entity.source_kind().as_str().to_string()),
                    Some(entity.source_instance().as_str().to_string()),
                    Some(entity.external_entity_id().as_str().to_string()),
                )
            });
    let (order_mode, order_domain, order_value) = mapping::source_order(receipt.source_order());
    transaction
        .execute(
            domain_sql::INSERT_RECEIPT,
            params![
                mapping::id(receipt.id()),
                key.source_kind().as_str(),
                key.source_instance().as_str(),
                key.occurrence_id().as_str(),
                entity_kind,
                entity_instance,
                external,
                mapping::fingerprint(receipt.fingerprint()),
                order_mode,
                order_domain,
                order_value,
                mapping::timestamp(receipt.occurred_at()),
                mapping::timestamp(receipt.ingested_at()),
                mapping::id(bundle.idempotency().key())
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)
}

async fn persist_entity(
    transaction: &Transaction<'_>,
    entity: &attention_kernel::SourceEntity,
) -> Result<(), PortError<Error>> {
    let (mode, domain, value) = mapping::source_order(entity.order());
    transaction
        .execute(
            domain_sql::INSERT_ENTITY,
            params![
                mapping::id(entity.id()),
                entity.key().source_kind().as_str(),
                entity.key().source_instance().as_str(),
                entity.key().external_entity_id().as_str(),
                mapping::counter(entity.version().value()),
                mapping::id(entity.latest_receipt_id()),
                mode,
                domain,
                value
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    Ok(())
}

pub async fn ingest_source(
    writer: &Writer,
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    bundle: IngestSourceOccurrenceBundle,
) -> Result<IngestSourceOccurrenceResult, PortError<Error>> {
    let identity = bundle.idempotency().clone();
    let occurrence = bundle.occurrence_guard().key().clone();
    let occurrence_fingerprint = bundle.occurrence_guard().fingerprint();
    #[cfg(test)]
    let semantic_failpoint = writer.semantic_failpoint();
    let result = writer
        .with_immediate(engine, move |transaction| {
            async move {
                let head = stream_head(transaction).await?;
                let cursor = next_cursor(head)?;
                let result = applied(bundle.value(), bundle.effects(), cursor);
                let prior = PriorMutationOutcome::IngestSourceOccurrence(result.clone());
                match begin(transaction, bundle.idempotency(), &prior).await? {
                    Begin::Replay(PriorMutationOutcome::IngestSourceOccurrence(stored)) => {
                        return Ok(TransactionDecision::Rollback(stored));
                    }
                    Begin::Replay(_) => {
                        return Err(PortError::Adapter(Error::CommitOutcomeUnknown));
                    }
                    Begin::Fresh(_) => {}
                }
                #[cfg(test)]
                crate::writer::semantic_fail(
                    semantic_failpoint,
                    crate::writer::SemanticFailpoint::AfterOutcome,
                )?;
                if insert_receipt(transaction, &bundle).await? == 0 {
                    let key = bundle.occurrence_guard().key();
                    let mut rows = transaction
                        .query(
                            domain_sql::SELECT_RECEIPT_BY_OCCURRENCE,
                            params![
                                key.source_kind().as_str(),
                                key.source_instance().as_str(),
                                key.occurrence_id().as_str()
                            ],
                        )
                        .await
                        .map_err(Error::from)
                        .map_err(PortError::Adapter)?;
                    let row = rows
                        .next()
                        .await
                        .map_err(Error::from)
                        .map_err(PortError::Adapter)?
                        .ok_or_else(|| PortError::Adapter(Error::CommitOutcomeUnknown))?;
                    let fingerprint = mapping::parse_fingerprint(
                        &decode::blob(&row, 7).map_err(PortError::Adapter)?,
                    )
                    .map_err(PortError::Adapter)?;
                    if fingerprint != bundle.occurrence_guard().fingerprint() {
                        return semantic(SemanticError::OccurrenceContentMismatch(key.clone()));
                    }
                    let accepted_key: attention_kernel::MutationIdempotencyKey =
                        mapping::parse_id(&decode::text(&row, 13).map_err(PortError::Adapter)?)
                            .map_err(PortError::Adapter)?;
                    drop(rows);
                    let mut outcome_rows = transaction
                        .query(
                            domain_sql::SELECT_OUTCOME,
                            params![mapping::id(accepted_key)],
                        )
                        .await
                        .map_err(Error::from)
                        .map_err(PortError::Adapter)?;
                    let outcome_row = outcome_rows
                        .next()
                        .await
                        .map_err(Error::from)
                        .map_err(PortError::Adapter)?
                        .ok_or_else(|| PortError::Adapter(Error::CommitOutcomeUnknown))?;
                    let stored = codec::decode_outcome_for_operation(
                        attention_kernel::MutationOperation::IngestSourceOccurrence,
                        decode::integer(&outcome_row, 2).map_err(PortError::Adapter)?,
                        &decode::blob(&outcome_row, 3).map_err(PortError::Adapter)?,
                    )
                    .map_err(PortError::Adapter)?;
                    drop(outcome_rows);
                    let PriorMutationOutcome::IngestSourceOccurrence(stored) = stored.replayed()
                    else {
                        return Err(PortError::Adapter(Error::CommitOutcomeUnknown));
                    };
                    return Ok(TransactionDecision::Rollback(stored));
                }
                let actual = if let Some(key) = bundle.authority_guard().key() {
                    let mut rows = transaction
                        .query(
                            domain_sql::SELECT_ENTITY,
                            params![
                                key.source_kind().as_str(),
                                key.source_instance().as_str(),
                                key.external_entity_id().as_str()
                            ],
                        )
                        .await
                        .map_err(Error::from)
                        .map_err(PortError::Adapter)?;
                    let value = rows
                        .next()
                        .await
                        .map_err(Error::from)
                        .map_err(PortError::Adapter)?
                        .map(|row| mapping::entity(&row))
                        .transpose()
                        .map_err(PortError::Adapter)?;
                    drop(rows);
                    value.map_or(ObservedSourceAuthority::Absent, |entity| {
                        entity.observed_authority()
                    })
                } else {
                    ObservedSourceAuthority::Absent
                };
                if &actual != bundle.authority_guard().observed() {
                    let key = bundle.authority_guard().key().cloned().ok_or_else(|| {
                        PortError::Adapter(Error::MigrationIntegrity("absent authority conflict"))
                    })?;
                    return semantic(SemanticError::ObservedSourceVersionConflict {
                        entity: key,
                        observed: bundle.authority_guard().observed().version(),
                        actual: actual.version(),
                    });
                }
                if let Some(entity) = bundle.entity() {
                    persist_entity(transaction, entity).await?;
                }
                if let Some(signal) = bundle.signal() {
                    insert_eventual_signal(transaction, signal).await?;
                }
                #[cfg(test)]
                crate::writer::semantic_fail(
                    semantic_failpoint,
                    crate::writer::SemanticFailpoint::AfterRoot,
                )?;
                finish(transaction, head, cursor, bundle.effects()).await?;
                #[cfg(test)]
                crate::writer::semantic_fail(
                    semantic_failpoint,
                    crate::writer::SemanticFailpoint::AfterFinish,
                )?;
                Ok(TransactionDecision::Commit(result))
            }
            .boxed()
        })
        .await;
    match result {
        Err(PortError::Adapter(Error::CommitOutcomeUnknown)) => {
            resolve_source(
                readers,
                engine,
                &identity,
                &occurrence,
                occurrence_fingerprint,
            )
            .await
        }
        other => other,
    }
}

fn current_fire(reminder: &Reminder) -> Option<ReminderFireId> {
    reminder
        .fires()
        .iter()
        .find(|fire| {
            matches!(
                fire.state(),
                ReminderFireState::Scheduled | ReminderFireState::Fired
            )
        })
        .map(attention_kernel::ReminderFire::id)
}

async fn persist_fires(
    transaction: &Transaction<'_>,
    reminder: &Reminder,
) -> Result<(), PortError<Error>> {
    for (ordinal, fire) in reminder.fires().iter().enumerate() {
        let ordinal = i64::try_from(ordinal).map_err(|_| {
            PortError::Adapter(Error::MigrationIntegrity("reminder fire ordinal overflow"))
        })?;
        transaction
            .execute(
                domain_sql::UPSERT_FIRE,
                params![
                    mapping::id(fire.id()),
                    mapping::id(reminder.id()),
                    ordinal,
                    mapping::timestamp(fire.trigger_at()),
                    mapping::reminder_fire_state(fire.state())
                ],
            )
            .await
            .map_err(Error::from)
            .map_err(PortError::Adapter)?;
    }
    Ok(())
}

async fn create_reminder_root(
    transaction: &Transaction<'_>,
    bundle: &CreateReminderBundle,
) -> Result<(), PortError<Error>> {
    let reminder = bundle.root();
    let (target_kind, target_id) = match reminder.target() {
        ReminderTarget::WorkItem(id) => (0, mapping::id(id)),
        ReminderTarget::AttentionSignal(id) => (1, mapping::id(id)),
    };
    let affected = transaction
        .execute(
            domain_sql::INSERT_REMINDER,
            params![
                mapping::id(reminder.id()),
                mapping::counter(reminder.revision().value()),
                target_kind,
                target_id,
                mapping::timestamp(reminder.trigger_at())
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    if affected == 0 {
        return semantic(SemanticError::CreateConflict(ResourceRef::Reminder(
            reminder.id(),
        )));
    }
    persist_fires(transaction, reminder).await?;
    let fire = current_fire(reminder).ok_or_else(|| {
        PortError::Adapter(Error::MigrationIntegrity(
            "created reminder has no current fire",
        ))
    })?;
    transaction
        .execute(
            domain_sql::SET_CURRENT_FIRE,
            params![mapping::id(reminder.id()), mapping::id(fire)],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    Ok(())
}

async fn update_reminder_root(
    transaction: &Transaction<'_>,
    reminder: &Reminder,
    guards: &ReminderMutationGuards,
) -> Result<(), PortError<Error>> {
    let (target_kind, target_id) = match reminder.target() {
        ReminderTarget::WorkItem(id) => (0, mapping::id(id)),
        ReminderTarget::AttentionSignal(id) => (1, mapping::id(id)),
    };
    persist_fires(transaction, reminder).await?;
    let affected = transaction
        .execute(
            domain_sql::UPDATE_REMINDER,
            params![
                mapping::id(reminder.id()),
                mapping::counter(reminder.revision().value()),
                target_kind,
                target_id,
                mapping::timestamp(reminder.trigger_at()),
                current_fire(reminder).map(mapping::id),
                mapping::counter(guards.revision().expected().value()),
                mapping::id(guards.current_fire().fire_id())
            ],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    if affected == 1 {
        return Ok(());
    }
    let mut rows = transaction
        .query(
            domain_sql::SELECT_REMINDER,
            params![mapping::id(reminder.id())],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    let Some(row) = rows
        .next()
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?
    else {
        return semantic(SemanticError::NotFound(ResourceRef::Reminder(
            reminder.id(),
        )));
    };
    let header = mapping::reminder_header(&row).map_err(PortError::Adapter)?;
    drop(rows);
    if header.revision != guards.revision().expected() {
        return semantic(SemanticError::ExpectedRevisionConflict {
            resource: ResourceRef::Reminder(reminder.id()),
            expected: guards.revision().expected(),
            actual: header.revision,
        });
    }
    semantic(SemanticError::NotFound(ResourceRef::ReminderFire(
        guards.current_fire().fire_id(),
    )))
}

root_commit!(
    create_reminder,
    CreateReminderBundle,
    CreateReminderResult,
    CreateReminder,
    create_reminder_root
);

async fn fire_reminder_root(
    transaction: &Transaction<'_>,
    bundle: &FireReminderBundle,
) -> Result<(), PortError<Error>> {
    update_reminder_root(transaction, bundle.root(), bundle.guard()).await
}

async fn acknowledge_reminder_root(
    transaction: &Transaction<'_>,
    bundle: &AcknowledgeReminderFireBundle,
) -> Result<(), PortError<Error>> {
    update_reminder_root(transaction, bundle.root(), bundle.guard()).await
}

async fn snooze_reminder_root(
    transaction: &Transaction<'_>,
    bundle: &SnoozeReminderFireBundle,
) -> Result<(), PortError<Error>> {
    update_reminder_root(transaction, bundle.root(), bundle.guard()).await
}

root_commit!(
    fire_reminder,
    FireReminderBundle,
    FireReminderResult,
    FireReminder,
    fire_reminder_root
);

root_commit!(
    acknowledge_reminder,
    AcknowledgeReminderFireBundle,
    AcknowledgeReminderFireResult,
    AcknowledgeReminderFire,
    acknowledge_reminder_root
);

root_commit!(
    snooze_reminder,
    SnoozeReminderFireBundle,
    SnoozeReminderFireResult,
    SnoozeReminderFire,
    snooze_reminder_root
);
