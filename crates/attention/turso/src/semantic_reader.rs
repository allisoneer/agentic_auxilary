use crate::Error;
use crate::codec;
use crate::decode;
use crate::domain_sql;
use crate::mapping;
use crate::reader::ReaderPool;
use attention_kernel::AttentionSignal;
use attention_kernel::AttentionSignalId;
use attention_kernel::AttentionSnapshot;
use attention_kernel::CanonicalFingerprint;
use attention_kernel::ChangeEvent;
use attention_kernel::ChangeEventId;
use attention_kernel::ChangeGap;
use attention_kernel::ChangePage;
use attention_kernel::ChangesAfterQuery;
use attention_kernel::ChangesResult;
use attention_kernel::CommitCursor;
use attention_kernel::DueReminderFire;
use attention_kernel::DueReminderFiresQuery;
use attention_kernel::MutationIdempotencyKey;
use attention_kernel::MutationOperation;
use attention_kernel::OccurrenceKey;
use attention_kernel::PriorMutationOutcome;
use attention_kernel::PriorMutationRecord;
use attention_kernel::Reminder;
use attention_kernel::ReminderId;
use attention_kernel::SourceAuthorityQuery;
use attention_kernel::SourceEntity;
use attention_kernel::SourceReceipt;
use attention_kernel::SourceReceiptId;
use attention_kernel::WorkItem;
use attention_kernel::WorkItemId;
use futures::FutureExt;
use tokio::sync::Mutex;
use turso_db::Database;
use turso_db::params;
use turso_db::transaction::Transaction;

async fn one<T>(
    transaction: &Transaction<'_>,
    sql: &str,
    parameters: impl turso_db::IntoParams,
    decode_row: impl FnOnce(&turso_db::Row) -> Result<T, Error>,
) -> Result<Option<T>, Error> {
    let mut rows = transaction.query(sql, parameters).await?;
    let value = rows.next().await?.map(|row| decode_row(&row)).transpose()?;
    drop(rows);
    Ok(value)
}

pub async fn work_item(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    id: WorkItemId,
) -> Result<Option<WorkItem>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            async move {
                one(
                    transaction,
                    domain_sql::SELECT_WORK_ITEM,
                    params![mapping::id(id)],
                    mapping::work_item,
                )
                .await
            }
            .boxed()
        })
        .await
}

pub async fn signal(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    id: AttentionSignalId,
) -> Result<Option<AttentionSignal>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            async move {
                one(
                    transaction,
                    domain_sql::SELECT_SIGNAL,
                    params![mapping::id(id)],
                    mapping::signal,
                )
                .await
            }
            .boxed()
        })
        .await
}

async fn reminder_in(
    transaction: &Transaction<'_>,
    id: ReminderId,
) -> Result<Option<Reminder>, Error> {
    let Some(header) = one(
        transaction,
        domain_sql::SELECT_REMINDER,
        params![mapping::id(id)],
        mapping::reminder_header,
    )
    .await?
    else {
        return Ok(None);
    };
    reminder_from_header(transaction, &header).await.map(Some)
}

async fn reminder_from_header(
    transaction: &Transaction<'_>,
    header: &mapping::ReminderHeader,
) -> Result<Reminder, Error> {
    let mut rows = transaction
        .query(
            domain_sql::SELECT_REMINDER_FIRES,
            params![mapping::id(header.id)],
        )
        .await?;
    let mut fires = Vec::new();
    while let Some(row) = rows.next().await? {
        fires.push(mapping::reminder_fire(&row)?);
    }
    drop(rows);
    mapping::reminder(header, fires)
}

pub async fn reminder(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    id: ReminderId,
) -> Result<Option<Reminder>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            reminder_in(transaction, id).boxed()
        })
        .await
}

pub async fn due_reminder_fires(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    query: DueReminderFiresQuery,
) -> Result<Vec<DueReminderFire>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            async move {
                let mut rows = transaction
                    .query(domain_sql::SELECT_DUE_REMINDER_FIRE_CANDIDATES, ())
                    .await?;
                let mut due = Vec::new();
                while let Some(row) = rows.next().await? {
                    let trigger_at = mapping::parse_timestamp(&decode::text(&row, 3)?)?;
                    if trigger_at <= *query.due_at_or_before() {
                        due.push(DueReminderFire::new(
                            mapping::parse_id(&decode::text(&row, 1)?)?,
                            mapping::parse_id(&decode::text(&row, 0)?)?,
                            attention_kernel::Revision::try_from(mapping::parse_counter(
                                &decode::blob(&row, 2)?,
                            )?)
                            .map_err(|error| Error::Decode(Box::new(error)))?,
                            trigger_at,
                        ));
                    }
                }
                drop(rows);
                due.sort_by_key(|fire| (*fire.trigger_at(), fire.fire_id(), fire.reminder_id()));
                due.truncate(query.limit().value());
                Ok(due)
            }
            .boxed()
        })
        .await
}

pub async fn entity(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    query: &SourceAuthorityQuery,
) -> Result<Option<SourceEntity>, Error> {
    let key = query.key().clone();
    readers
        .with_snapshot(engine, move |transaction| {
            let key = key.clone();
            async move {
                one(
                    transaction,
                    domain_sql::SELECT_ENTITY,
                    params![
                        key.source_kind().as_str(),
                        key.source_instance().as_str(),
                        key.external_entity_id().as_str()
                    ],
                    mapping::entity,
                )
                .await
            }
            .boxed()
        })
        .await
}

pub async fn receipt(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    id: SourceReceiptId,
) -> Result<Option<SourceReceipt>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            async move {
                one(
                    transaction,
                    domain_sql::SELECT_RECEIPT,
                    params![mapping::id(id)],
                    mapping::receipt,
                )
                .await
            }
            .boxed()
        })
        .await
}

pub async fn outcome(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    key: MutationIdempotencyKey,
) -> Result<Option<PriorMutationOutcome>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            async move {
                one(
                    transaction,
                    domain_sql::SELECT_OUTCOME,
                    params![mapping::id(key)],
                    |row| {
                        let operation = mapping::parse_operation(decode::integer(row, 0)?)?;
                        let version = decode::integer(row, 2)?;
                        let bytes = decode::blob(row, 3)?;
                        codec::decode_outcome_for_operation(operation, version, &bytes)
                    },
                )
                .await
            }
            .boxed()
        })
        .await
}

pub async fn prior_mutation(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    key: MutationIdempotencyKey,
) -> Result<Option<PriorMutationRecord>, Error> {
    Ok(stored_outcome(readers, engine, key).await?.map(|stored| {
        PriorMutationRecord::new(stored.operation, stored.fingerprint, stored.outcome)
    }))
}

pub async fn change_event(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    id: ChangeEventId,
) -> Result<Option<ChangeEvent>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            async move {
                one(
                    transaction,
                    domain_sql::SELECT_CHANGE_EVENT,
                    params![mapping::id(id)],
                    |row| {
                        let cursor =
                            CommitCursor::try_from(mapping::parse_counter(&decode::blob(row, 0)?)?)
                                .map_err(|error| Error::Decode(Box::new(error)))?;
                        codec::decode_event(
                            cursor,
                            mapping::parse_id(&decode::text(row, 1)?)?,
                            mapping::parse_timestamp(&decode::text(row, 2)?)?,
                            mapping::parse_change_kind(decode::integer(row, 3)?)?,
                            decode::integer(row, 4)?,
                            &decode::blob(row, 5)?,
                        )
                    },
                )
                .await
            }
            .boxed()
        })
        .await
}

pub struct StoredOutcome {
    pub operation: MutationOperation,
    pub fingerprint: CanonicalFingerprint,
    pub outcome: PriorMutationOutcome,
}

pub struct StoredOccurrenceOutcome {
    pub fingerprint: CanonicalFingerprint,
    pub outcome: PriorMutationOutcome,
}

pub async fn stored_occurrence_outcome(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    key: &OccurrenceKey,
) -> Result<Option<StoredOccurrenceOutcome>, Error> {
    let key = key.clone();
    readers
        .with_snapshot(engine, move |transaction| {
            let key = key.clone();
            async move {
                let mut receipt_rows = transaction
                    .query(
                        domain_sql::SELECT_RECEIPT_BY_OCCURRENCE,
                        params![
                            key.source_kind().as_str(),
                            key.source_instance().as_str(),
                            key.occurrence_id().as_str()
                        ],
                    )
                    .await?;
                let Some(receipt_row) = receipt_rows.next().await? else {
                    return Ok(None);
                };
                let fingerprint = mapping::parse_fingerprint(&decode::blob(&receipt_row, 7)?)?;
                let accepted_key: MutationIdempotencyKey =
                    mapping::parse_id(&decode::text(&receipt_row, 13)?)?;
                drop(receipt_rows);

                let outcome = one(
                    transaction,
                    domain_sql::SELECT_OUTCOME,
                    params![mapping::id(accepted_key)],
                    |row| {
                        let operation = mapping::parse_operation(decode::integer(row, 0)?)?;
                        let version = decode::integer(row, 2)?;
                        let bytes = decode::blob(row, 3)?;
                        codec::decode_outcome_for_operation(operation, version, &bytes)
                    },
                )
                .await?
                .ok_or_else(|| {
                    Error::Decode(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "accepted occurrence outcome is missing",
                    )))
                })?;
                Ok(Some(StoredOccurrenceOutcome {
                    fingerprint,
                    outcome,
                }))
            }
            .boxed()
        })
        .await
}

pub async fn stored_outcome(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    key: MutationIdempotencyKey,
) -> Result<Option<StoredOutcome>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            async move {
                one(
                    transaction,
                    domain_sql::SELECT_OUTCOME,
                    params![mapping::id(key)],
                    |row| {
                        let version = decode::integer(row, 2)?;
                        let bytes = decode::blob(row, 3)?;
                        let operation = mapping::parse_operation(decode::integer(row, 0)?)?;
                        Ok(StoredOutcome {
                            operation,
                            fingerprint: mapping::parse_fingerprint(&decode::blob(row, 1)?)?,
                            outcome: codec::decode_outcome_for_operation(
                                operation, version, &bytes,
                            )?,
                        })
                    },
                )
                .await
            }
            .boxed()
        })
        .await
}

async fn all_work_items(transaction: &Transaction<'_>) -> Result<Vec<WorkItem>, Error> {
    let mut rows = transaction.query(domain_sql::SELECT_WORK_ITEMS, ()).await?;
    let mut values = Vec::new();
    while let Some(row) = rows.next().await? {
        values.push(mapping::work_item(&row)?);
    }
    drop(rows);
    Ok(values)
}

async fn all_signals(transaction: &Transaction<'_>) -> Result<Vec<AttentionSignal>, Error> {
    let mut rows = transaction.query(domain_sql::SELECT_SIGNALS, ()).await?;
    let mut values = Vec::new();
    while let Some(row) = rows.next().await? {
        values.push(mapping::signal(&row)?);
    }
    drop(rows);
    Ok(values)
}

async fn all_reminders(transaction: &Transaction<'_>) -> Result<Vec<Reminder>, Error> {
    let mut rows = transaction.query(domain_sql::SELECT_REMINDERS, ()).await?;
    let mut headers = Vec::new();
    while let Some(row) = rows.next().await? {
        headers.push(mapping::reminder_header(&row)?);
    }
    drop(rows);
    let mut reminders = Vec::with_capacity(headers.len());
    for header in headers {
        reminders.push(reminder_from_header(transaction, &header).await?);
    }
    Ok(reminders)
}

async fn stream_state(
    transaction: &Transaction<'_>,
) -> Result<(CommitCursor, CommitCursor), Error> {
    one(transaction, domain_sql::SELECT_STREAM_STATE, (), |row| {
        let head = CommitCursor::try_from(mapping::parse_counter(&decode::blob(row, 0)?)?)
            .map_err(|error| Error::Decode(Box::new(error)))?;
        let floor = CommitCursor::try_from(mapping::parse_counter(&decode::blob(row, 1)?)?)
            .map_err(|error| Error::Decode(Box::new(error)))?;
        Ok((head, floor))
    })
    .await?
    .ok_or_else(|| {
        Error::Decode(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stream state row is missing",
        )))
    })
}

pub async fn snapshot(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
) -> Result<AttentionSnapshot, Error> {
    readers
        .with_snapshot(engine, |transaction| {
            async move {
                let (head, _) = stream_state(transaction).await?;
                Ok(AttentionSnapshot::new(
                    head,
                    all_work_items(transaction).await?,
                    all_signals(transaction).await?,
                    all_reminders(transaction).await?,
                ))
            }
            .boxed()
        })
        .await
}

pub async fn changes(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    query: ChangesAfterQuery,
) -> Result<ChangesResult, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            async move {
                let (head, floor) = stream_state(transaction).await?;
                if query.after() < floor {
                    return Ok(ChangesResult::Gap(ChangeGap::Expired {
                        requested_after: query.after(),
                        earliest_available: floor,
                        latest_available: head,
                    }));
                }
                if query.after() > head {
                    return Ok(ChangesResult::Gap(ChangeGap::Future {
                        requested_after: query.after(),
                        latest_available: head,
                    }));
                }
                let limit = i64::try_from(query.limit().value())
                    .ok()
                    .and_then(|value| value.checked_add(1))
                    .ok_or_else(|| {
                        Error::Decode(Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "change page limit exceeds database range",
                        )))
                    })?;
                let mut rows = transaction
                    .query(
                        domain_sql::SELECT_CHANGES,
                        params![
                            mapping::counter(query.after().value()),
                            mapping::counter(head.value()),
                            limit
                        ],
                    )
                    .await?;
                let mut events = Vec::new();
                while let Some(row) = rows.next().await? {
                    let cursor =
                        CommitCursor::try_from(mapping::parse_counter(&decode::blob(&row, 0)?)?)
                            .map_err(|error| Error::Decode(Box::new(error)))?;
                    events.push(codec::decode_event(
                        cursor,
                        mapping::parse_id(&decode::text(&row, 1)?)?,
                        mapping::parse_timestamp(&decode::text(&row, 2)?)?,
                        mapping::parse_change_kind(decode::integer(&row, 3)?)?,
                        decode::integer(&row, 4)?,
                        &decode::blob(&row, 5)?,
                    )?);
                }
                drop(rows);
                let has_more = events.len() > query.limit().value();
                events.truncate(query.limit().value());
                let resume = events
                    .last()
                    .map_or_else(|| query.after(), attention_kernel::ChangeEvent::cursor);
                Ok(ChangesResult::Page(ChangePage::new(
                    events, resume, has_more,
                )))
            }
            .boxed()
        })
        .await
}
