use crate::Error;
use crate::decode;
use crate::delivery_reader;
use crate::domain_sql;
use crate::mapping;
use crate::reader::ReaderPool;
use crate::writer::TransactionDecision;
use crate::writer::Writer;
use attention_kernel::BoundedDeliveryText;
use attention_kernel::CheckpointAdvance;
use attention_kernel::CheckpointAdvanceOutcome;
use attention_kernel::ClaimOutcome;
use attention_kernel::CommitCursor;
use attention_kernel::DeliveryClaim;
use attention_kernel::DeliveryClaimQuery;
use attention_kernel::DeliveryCompletionOutcome;
use attention_kernel::DeliveryLeaseToken;
use attention_kernel::DeliveryState;
use attention_kernel::DeliveryStatus;
use attention_kernel::OutboxIntentId;
use attention_kernel::PortError;
use attention_kernel::ProviderMessageId;
use attention_kernel::RenewOutcome;
use chrono::DateTime;
use chrono::Utc;
use futures::FutureExt;
use std::collections::HashSet;
use tokio::sync::Mutex;
use turso_db::Database;
use turso_db::params;
use turso_db::transaction::Transaction;

struct Candidate {
    intent_id: OutboxIntentId,
    created_at: DateTime<Utc>,
    status: DeliveryStatus,
}

fn eligible(status: &DeliveryStatus, eligible_at: &DateTime<Utc>) -> bool {
    match status {
        DeliveryStatus::Pending => true,
        DeliveryStatus::Leased { expires_at, .. } => expires_at <= eligible_at,
        DeliveryStatus::Retryable { next_retry_at, .. } => next_retry_at <= eligible_at,
        DeliveryStatus::Succeeded { .. }
        | DeliveryStatus::Skipped { .. }
        | DeliveryStatus::TerminalFailure { .. } => false,
    }
}

fn fresh_token(
    issued: &mut HashSet<DeliveryLeaseToken>,
    previous: Option<DeliveryLeaseToken>,
) -> Result<DeliveryLeaseToken, PortError<Error>> {
    loop {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes)
            .map_err(|error| PortError::Adapter(Error::Engine(Box::new(error))))?;
        let token = DeliveryLeaseToken::from_bytes(bytes);
        if previous == Some(token) || !issued.insert(token) {
            continue;
        }
        return Ok(token);
    }
}

async fn state_in(
    transaction: &Transaction<'_>,
    intent_id: OutboxIntentId,
) -> Result<Option<DeliveryStatus>, PortError<Error>> {
    let mut rows = transaction
        .query(
            domain_sql::SELECT_DELIVERY_STATE,
            params![mapping::id(intent_id)],
        )
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    let status = rows
        .next()
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?
        .map(|row| mapping::delivery_status(&row, 0))
        .transpose()
        .map_err(PortError::Adapter)?;
    drop(rows);
    Ok(status)
}

async fn checkpoint_in(
    transaction: &Transaction<'_>,
    worker: &str,
) -> Result<Option<CommitCursor>, PortError<Error>> {
    let mut rows = transaction
        .query(domain_sql::SELECT_DELIVERY_CHECKPOINT, params![worker])
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?;
    let cursor = match rows
        .next()
        .await
        .map_err(Error::from)
        .map_err(PortError::Adapter)?
    {
        Some(row) => Some(
            mapping::parse_checkpoint_cursor(&decode::blob(&row, 1).map_err(PortError::Adapter)?)
                .map_err(PortError::Adapter)?,
        ),
        None => None,
    };
    drop(rows);
    Ok(cursor)
}

async fn prove_status(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    intent_id: OutboxIntentId,
    expected: &DeliveryStatus,
) -> Result<bool, PortError<Error>> {
    Ok(delivery_reader::status(readers, engine, intent_id)
        .await
        .map_err(PortError::Adapter)?
        .is_some_and(|status| status == *expected))
}

pub async fn claim(
    writer: &Writer,
    engine: &Mutex<Option<Database>>,
    query: DeliveryClaimQuery,
) -> Result<Vec<DeliveryClaim>, PortError<Error>> {
    let eligible_at = *query.eligible_at();
    let lease_expires_at = *query.lease_expires_at();
    let limit = query.limit().value();
    writer
        .with_immediate(engine, move |transaction| {
            async move {
                let mut rows = transaction
                    .query(domain_sql::SELECT_DELIVERY_CANDIDATES, ())
                    .await
                    .map_err(Error::from)
                    .map_err(PortError::Adapter)?;
                let mut candidates = Vec::new();
                while let Some(row) = rows
                    .next()
                    .await
                    .map_err(Error::from)
                    .map_err(PortError::Adapter)?
                {
                    candidates.push(Candidate {
                        intent_id: mapping::parse_id(
                            &decode::text(&row, 0).map_err(PortError::Adapter)?,
                        )
                        .map_err(PortError::Adapter)?,
                        created_at: mapping::parse_timestamp(
                            &decode::text(&row, 1).map_err(PortError::Adapter)?,
                        )
                        .map_err(PortError::Adapter)?,
                        status: mapping::delivery_status(&row, 2).map_err(PortError::Adapter)?,
                    });
                }
                drop(rows);
                candidates.retain(|candidate| eligible(&candidate.status, &eligible_at));
                candidates.sort_by_key(|candidate| (candidate.created_at, candidate.intent_id));
                candidates.truncate(limit);
                if candidates.is_empty() {
                    return Ok(TransactionDecision::Rollback(Vec::new()));
                }

                let mut issued = HashSet::new();
                let mut claims = Vec::with_capacity(candidates.len());
                for candidate in candidates {
                    let previous = match &candidate.status {
                        DeliveryStatus::Leased { token, .. } => Some(*token),
                        _ => None,
                    };
                    let token = fresh_token(&mut issued, previous)?;
                    let mut state =
                        DeliveryState::reconstruct(candidate.intent_id, candidate.status);
                    let ClaimOutcome::Claimed(claim) = state.claim(token, lease_expires_at) else {
                        return Err(PortError::Adapter(Error::InvariantViolation(
                            "eligible delivery became terminal inside claim transaction",
                        )));
                    };
                    let affected = transaction
                        .execute(
                            domain_sql::UPDATE_DELIVERY_LEASED,
                            params![
                                mapping::id(candidate.intent_id),
                                mapping::lease_token(token),
                                mapping::timestamp(&lease_expires_at)
                            ],
                        )
                        .await
                        .map_err(Error::from)
                        .map_err(PortError::Adapter)?;
                    if affected != 1 {
                        return Err(PortError::Adapter(Error::InvariantViolation(
                            "delivery claim update lost authority",
                        )));
                    }
                    claims.push(claim);
                }
                Ok(TransactionDecision::Commit(claims))
            }
            .boxed()
        })
        .await
}

pub async fn renew(
    writer: &Writer,
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    intent_id: OutboxIntentId,
    token: DeliveryLeaseToken,
    expires_at: DateTime<Utc>,
) -> Result<RenewOutcome, PortError<Error>> {
    let attempted = writer
        .with_immediate(engine, move |transaction| {
            async move {
                let Some(status) = state_in(transaction, intent_id).await? else {
                    return Ok(TransactionDecision::Rollback(RenewOutcome::NotLeased));
                };
                if matches!(status, DeliveryStatus::Leased { token: current, expires_at: current_at }
                    if current == token && current_at == expires_at)
                {
                    return Ok(TransactionDecision::Rollback(RenewOutcome::Renewed));
                }
                let mut state = DeliveryState::reconstruct(intent_id, status);
                let outcome = state.renew(token, expires_at);
                if outcome != RenewOutcome::Renewed {
                    return Ok(TransactionDecision::Rollback(outcome));
                }
                let affected = transaction
                    .execute(
                        domain_sql::UPDATE_DELIVERY_RENEWED,
                        params![
                            mapping::id(intent_id),
                            mapping::lease_token(token),
                            mapping::timestamp(&expires_at)
                        ],
                    )
                    .await
                    .map_err(Error::from)
                    .map_err(PortError::Adapter)?;
                if affected != 1 {
                    return Err(PortError::Adapter(Error::InvariantViolation(
                        "delivery renewal lost authority",
                    )));
                }
                Ok(TransactionDecision::Commit(RenewOutcome::Renewed))
            }
            .boxed()
        })
        .await;
    match attempted {
        Err(PortError::Adapter(Error::CommitOutcomeUnknown)) => {
            let expected = DeliveryStatus::Leased { token, expires_at };
            if prove_status(readers, engine, intent_id, &expected).await? {
                Ok(RenewOutcome::Renewed)
            } else {
                Err(PortError::Adapter(Error::CommitOutcomeUnknown))
            }
        }
        result => result,
    }
}

pub async fn succeed(
    writer: &Writer,
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    intent_id: OutboxIntentId,
    token: DeliveryLeaseToken,
    provider_message_id: ProviderMessageId,
    succeeded_at: DateTime<Utc>,
) -> Result<DeliveryCompletionOutcome, PortError<Error>> {
    let provider_text = mapping::provider_message_id(&provider_message_id)
        .map_err(PortError::Adapter)?
        .to_owned();
    let expected = DeliveryStatus::Succeeded {
        provider_message_id: provider_message_id.clone(),
        succeeded_at,
    };
    let attempted = writer
        .with_immediate(engine, move |transaction| {
            async move {
                let Some(status) = state_in(transaction, intent_id).await? else {
                    return Ok(TransactionDecision::Rollback(
                        DeliveryCompletionOutcome::Fenced,
                    ));
                };
                let mut state = DeliveryState::reconstruct(intent_id, status);
                let outcome = state.succeed(token, provider_message_id, succeeded_at);
                if outcome != DeliveryCompletionOutcome::Applied {
                    return Ok(TransactionDecision::Rollback(outcome));
                }
                let affected = transaction
                    .execute(
                        domain_sql::UPDATE_DELIVERY_SUCCEEDED,
                        params![
                            mapping::id(intent_id),
                            mapping::lease_token(token),
                            provider_text,
                            mapping::timestamp(&succeeded_at)
                        ],
                    )
                    .await
                    .map_err(Error::from)
                    .map_err(PortError::Adapter)?;
                if affected != 1 {
                    return Err(PortError::Adapter(Error::InvariantViolation(
                        "delivery success lost authority",
                    )));
                }
                Ok(TransactionDecision::Commit(
                    DeliveryCompletionOutcome::Applied,
                ))
            }
            .boxed()
        })
        .await;
    resolve_completion(attempted, readers, engine, intent_id, &expected).await
}

pub async fn fail_retryable(
    writer: &Writer,
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    intent_id: OutboxIntentId,
    token: DeliveryLeaseToken,
    failure: (u32, BoundedDeliveryText, DateTime<Utc>),
) -> Result<DeliveryCompletionOutcome, PortError<Error>> {
    let (attempt, error, next_retry_at) = failure;
    let error_text = mapping::delivery_text(&error)
        .map_err(PortError::Adapter)?
        .to_owned();
    let expected = DeliveryStatus::Retryable {
        attempt,
        error: error.clone(),
        next_retry_at,
    };
    let attempted = writer
        .with_immediate(engine, move |transaction| {
            async move {
                let Some(status) = state_in(transaction, intent_id).await? else {
                    return Ok(TransactionDecision::Rollback(
                        DeliveryCompletionOutcome::Fenced,
                    ));
                };
                let mut state = DeliveryState::reconstruct(intent_id, status);
                let outcome = state.fail_retryable(token, attempt, error, next_retry_at);
                if outcome != DeliveryCompletionOutcome::Applied {
                    return Ok(TransactionDecision::Rollback(outcome));
                }
                let affected = transaction
                    .execute(
                        domain_sql::UPDATE_DELIVERY_RETRYABLE,
                        params![
                            mapping::id(intent_id),
                            mapping::lease_token(token),
                            i64::from(attempt),
                            error_text,
                            mapping::timestamp(&next_retry_at)
                        ],
                    )
                    .await
                    .map_err(Error::from)
                    .map_err(PortError::Adapter)?;
                if affected != 1 {
                    return Err(PortError::Adapter(Error::InvariantViolation(
                        "retryable delivery failure lost authority",
                    )));
                }
                Ok(TransactionDecision::Commit(
                    DeliveryCompletionOutcome::Applied,
                ))
            }
            .boxed()
        })
        .await;
    resolve_completion(attempted, readers, engine, intent_id, &expected).await
}

pub async fn fail_terminal(
    writer: &Writer,
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    intent_id: OutboxIntentId,
    token: DeliveryLeaseToken,
    failure: (u32, BoundedDeliveryText, DateTime<Utc>),
) -> Result<DeliveryCompletionOutcome, PortError<Error>> {
    let (attempt, error, failed_at) = failure;
    let error_text = mapping::delivery_text(&error)
        .map_err(PortError::Adapter)?
        .to_owned();
    let expected = DeliveryStatus::TerminalFailure {
        attempt,
        error: error.clone(),
        failed_at,
    };
    let attempted = writer
        .with_immediate(engine, move |transaction| {
            async move {
                let Some(status) = state_in(transaction, intent_id).await? else {
                    return Ok(TransactionDecision::Rollback(
                        DeliveryCompletionOutcome::Fenced,
                    ));
                };
                let mut state = DeliveryState::reconstruct(intent_id, status);
                let outcome = state.fail_terminal(token, attempt, error, failed_at);
                if outcome != DeliveryCompletionOutcome::Applied {
                    return Ok(TransactionDecision::Rollback(outcome));
                }
                let affected = transaction
                    .execute(
                        domain_sql::UPDATE_DELIVERY_TERMINAL_FAILURE,
                        params![
                            mapping::id(intent_id),
                            mapping::lease_token(token),
                            i64::from(attempt),
                            error_text,
                            mapping::timestamp(&failed_at)
                        ],
                    )
                    .await
                    .map_err(Error::from)
                    .map_err(PortError::Adapter)?;
                if affected != 1 {
                    return Err(PortError::Adapter(Error::InvariantViolation(
                        "terminal delivery failure lost authority",
                    )));
                }
                Ok(TransactionDecision::Commit(
                    DeliveryCompletionOutcome::Applied,
                ))
            }
            .boxed()
        })
        .await;
    resolve_completion(attempted, readers, engine, intent_id, &expected).await
}

pub async fn skip(
    writer: &Writer,
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    intent_id: OutboxIntentId,
    token: DeliveryLeaseToken,
    reason: BoundedDeliveryText,
    skipped_at: DateTime<Utc>,
) -> Result<DeliveryCompletionOutcome, PortError<Error>> {
    let reason_text = mapping::delivery_text(&reason)
        .map_err(PortError::Adapter)?
        .to_owned();
    let expected = DeliveryStatus::Skipped {
        reason: reason.clone(),
        skipped_at,
    };
    let attempted = writer
        .with_immediate(engine, move |transaction| {
            async move {
                let Some(status) = state_in(transaction, intent_id).await? else {
                    return Ok(TransactionDecision::Rollback(
                        DeliveryCompletionOutcome::Fenced,
                    ));
                };
                let mut state = DeliveryState::reconstruct(intent_id, status);
                let outcome = state.skip(token, reason, skipped_at);
                if outcome != DeliveryCompletionOutcome::Applied {
                    return Ok(TransactionDecision::Rollback(outcome));
                }
                let affected = transaction
                    .execute(
                        domain_sql::UPDATE_DELIVERY_SKIPPED,
                        params![
                            mapping::id(intent_id),
                            mapping::lease_token(token),
                            reason_text,
                            mapping::timestamp(&skipped_at)
                        ],
                    )
                    .await
                    .map_err(Error::from)
                    .map_err(PortError::Adapter)?;
                if affected != 1 {
                    return Err(PortError::Adapter(Error::InvariantViolation(
                        "delivery skip lost authority",
                    )));
                }
                Ok(TransactionDecision::Commit(
                    DeliveryCompletionOutcome::Applied,
                ))
            }
            .boxed()
        })
        .await;
    resolve_completion(attempted, readers, engine, intent_id, &expected).await
}

async fn resolve_completion(
    attempted: Result<DeliveryCompletionOutcome, PortError<Error>>,
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    intent_id: OutboxIntentId,
    expected: &DeliveryStatus,
) -> Result<DeliveryCompletionOutcome, PortError<Error>> {
    match attempted {
        Err(PortError::Adapter(Error::CommitOutcomeUnknown)) => {
            if prove_status(readers, engine, intent_id, expected).await? {
                Ok(DeliveryCompletionOutcome::Applied)
            } else {
                Err(PortError::Adapter(Error::CommitOutcomeUnknown))
            }
        }
        result => result,
    }
}

pub async fn advance_checkpoint(
    writer: &Writer,
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    advance: CheckpointAdvance,
) -> Result<CheckpointAdvanceOutcome, PortError<Error>> {
    let worker = mapping::delivery_text(advance.worker())
        .map_err(PortError::Adapter)?
        .to_owned();
    let terminal_intent_id = advance.terminal_intent_id();
    let expected = advance.expected();
    let next = advance.next();
    let worker_for_write = worker.clone();
    let attempted = writer
        .with_immediate(engine, move |transaction| {
            async move {
                let terminal = state_in(transaction, terminal_intent_id)
                    .await?
                    .is_some_and(|status| status.is_terminal());
                if !terminal {
                    return Ok(TransactionDecision::Rollback(
                        CheckpointAdvanceOutcome::TerminalStateRequired,
                    ));
                }
                let current = checkpoint_in(transaction, &worker_for_write).await?;
                if current == Some(next) {
                    return Ok(TransactionDecision::Rollback(
                        CheckpointAdvanceOutcome::Repeated,
                    ));
                }
                if current != expected {
                    return Ok(TransactionDecision::Rollback(
                        CheckpointAdvanceOutcome::Conflict,
                    ));
                }
                let affected = match expected {
                    None => {
                        transaction
                            .execute(
                                domain_sql::INSERT_DELIVERY_CHECKPOINT,
                                params![worker_for_write, mapping::checkpoint_cursor(next)],
                            )
                            .await
                    }
                    Some(expected) => {
                        transaction
                            .execute(
                                domain_sql::UPDATE_DELIVERY_CHECKPOINT,
                                params![
                                    worker_for_write,
                                    mapping::checkpoint_cursor(expected),
                                    mapping::checkpoint_cursor(next)
                                ],
                            )
                            .await
                    }
                }
                .map_err(Error::from)
                .map_err(PortError::Adapter)?;
                if affected != 1 {
                    return Err(PortError::Adapter(Error::InvariantViolation(
                        "checkpoint compare-and-swap lost authority",
                    )));
                }
                Ok(TransactionDecision::Commit(
                    CheckpointAdvanceOutcome::Advanced,
                ))
            }
            .boxed()
        })
        .await;
    match attempted {
        Err(PortError::Adapter(Error::CommitOutcomeUnknown)) => {
            let cursor = delivery_reader::checkpoint_cursor(readers, engine, worker)
                .await
                .map_err(PortError::Adapter)?;
            let terminal = delivery_reader::status(readers, engine, terminal_intent_id)
                .await
                .map_err(PortError::Adapter)?
                .is_some_and(|status| status.is_terminal());
            if cursor == Some(next) && terminal {
                Ok(CheckpointAdvanceOutcome::Advanced)
            } else {
                Err(PortError::Adapter(Error::CommitOutcomeUnknown))
            }
        }
        result => result,
    }
}
