use crate::BusyTimeout;
use crate::Error;
use crate::decode;
use crate::migration;
use crate::migration::MigrationReport;
use crate::sql;
use attention_kernel::PortError;
use futures::future::BoxFuture;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;
use tokio::sync::Mutex;
use turso_db::Connection;
use turso_db::Database;
use turso_db::params;
use turso_db::transaction::Transaction;
use turso_db::transaction::TransactionBehavior;

pub enum TransactionDecision<T> {
    Commit(T),
    Rollback(T),
}

#[derive(Debug)]
pub struct Writer {
    connection: Mutex<Option<Connection>>,
    gate: Mutex<()>,
    busy_timeout: BusyTimeout,
    phase: AtomicU8,
    #[cfg(test)]
    semantic_failpoint: AtomicU8,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SemanticFailpoint {
    Disabled = 0,
    AfterOutcome = 1,
    AfterRoot = 2,
    AfterFinish = 3,
    AfterCommit = 4,
}

#[cfg(test)]
pub fn semantic_fail(
    configured: SemanticFailpoint,
    failpoint: SemanticFailpoint,
) -> Result<(), PortError<Error>> {
    if configured == failpoint {
        return Err(PortError::Adapter(Error::Engine(Box::new(
            std::io::Error::other("injected semantic write failure"),
        ))));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommitPhase {
    BeforeTransaction = 0,
    DuringEffects = 1,
    BeforeCommit = 2,
    CommitInvoked = 3,
    DefiniteResult = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeWriteOutcome {
    Applied,
    Replayed,
}

impl Writer {
    pub(crate) async fn with_immediate<T, F>(
        &self,
        engine: &Mutex<Option<Database>>,
        operation: F,
    ) -> Result<T, PortError<Error>>
    where
        F: for<'transaction, 'connection> FnOnce(
            &'transaction Transaction<'connection>,
        ) -> BoxFuture<
            'transaction,
            Result<TransactionDecision<T>, PortError<Error>>,
        >,
    {
        let _gate = self.gate.lock().await;
        self.set_phase(CommitPhase::BeforeTransaction);
        let mut connection = self
            .take_or_connect(engine)
            .await
            .map_err(PortError::Adapter)?;
        let transaction = match connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
        {
            Ok(transaction) => transaction,
            Err(error) => {
                let error = Error::from(error);
                return Err(PortError::Adapter(error));
            }
        };
        self.set_phase(CommitPhase::DuringEffects);
        match operation(&transaction).await {
            Ok(TransactionDecision::Rollback(value)) => {
                transaction
                    .rollback()
                    .await
                    .map_err(Error::from)
                    .map_err(PortError::Adapter)?;
                self.set_phase(CommitPhase::DefiniteResult);
                self.return_if_clean(connection).await;
                Ok(value)
            }
            Err(error) => {
                let rollback = transaction.rollback().await.map_err(Error::from);
                if rollback.is_ok() {
                    self.return_if_clean(connection).await;
                }
                Err(error)
            }
            Ok(TransactionDecision::Commit(value)) => {
                // No await separates these stores, so BeforeCommit is transient here. It is a
                // stable cooperative-task observation point only where an explicit barrier pauses.
                self.set_phase(CommitPhase::BeforeCommit);
                self.set_phase(CommitPhase::CommitInvoked);
                if transaction.commit().await.is_err() {
                    return Err(PortError::Adapter(Error::CommitOutcomeUnknown));
                }
                #[cfg(test)]
                if self.semantic_failpoint.load(Ordering::Acquire)
                    == SemanticFailpoint::AfterCommit as u8
                {
                    return Err(PortError::Adapter(Error::CommitOutcomeUnknown));
                }
                self.set_phase(CommitPhase::DefiniteResult);
                self.return_if_clean(connection).await;
                Ok(value)
            }
        }
    }

    pub(crate) fn new(connection: Connection, busy_timeout: BusyTimeout) -> Arc<Self> {
        Arc::new(Self {
            connection: Mutex::new(Some(connection)),
            gate: Mutex::new(()),
            busy_timeout,
            phase: AtomicU8::new(CommitPhase::BeforeTransaction as u8),
            #[cfg(test)]
            semantic_failpoint: AtomicU8::new(SemanticFailpoint::Disabled as u8),
        })
    }

    pub(crate) async fn write_probe(
        &self,
        engine: &Mutex<Option<Database>>,
        operation_id: &str,
        fingerprint: &[u8],
        value: &[u8],
    ) -> Result<ProbeWriteOutcome, Error> {
        self.write_probe_inner(engine, operation_id, fingerprint, value, None)
            .await
    }

    pub(crate) async fn run_migrations(
        &self,
        engine: &Mutex<Option<Database>>,
    ) -> Result<MigrationReport, Error> {
        let _gate = self.gate.lock().await;
        let mut connection = self.take_or_connect(engine).await?;
        let result = migration::run(&mut connection).await;
        if !matches!(&result, Err(Error::CommitOutcomeUnknown)) {
            self.return_if_clean(connection).await;
        }
        result
    }

    async fn write_probe_inner(
        &self,
        engine: &Mutex<Option<Database>>,
        operation_id: &str,
        fingerprint: &[u8],
        value: &[u8],
        pause_after_effect: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    ) -> Result<ProbeWriteOutcome, Error> {
        let _gate = self.gate.lock().await;
        self.set_phase(CommitPhase::BeforeTransaction);
        let mut connection = self.take_or_connect(engine).await?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(Error::from)?;
        self.set_phase(CommitPhase::DuringEffects);
        let affected = match transaction
            .execute(
                sql::UPSERT_QUALIFICATION_PROBE,
                params![operation_id, fingerprint.to_vec(), value.to_vec()],
            )
            .await
        {
            Ok(affected) => affected,
            Err(error) => {
                let mapped = Error::from(error);
                let _ = transaction.rollback().await;
                self.return_if_clean(connection).await;
                return Err(mapped);
            }
        };
        let outcome = if affected == 0 {
            let replay: Result<ProbeWriteOutcome, Error> = async {
                let mut rows = transaction
                    .query(sql::SELECT_QUALIFICATION_PROBE, params![operation_id])
                    .await?;
                let row = rows.next().await?.ok_or(Error::ProbeIdentityConflict)?;
                let stored_fingerprint = decode::blob(&row, 0)?;
                let stored_value = decode::blob(&row, 1)?;
                if stored_fingerprint != fingerprint || stored_value != value {
                    return Err(Error::ProbeIdentityConflict);
                }
                Ok(ProbeWriteOutcome::Replayed)
            }
            .await;
            match replay {
                Ok(outcome) => outcome,
                Err(error) => {
                    let _ = transaction.rollback().await;
                    self.return_if_clean(connection).await;
                    return Err(error);
                }
            }
        } else {
            ProbeWriteOutcome::Applied
        };
        self.set_phase(CommitPhase::BeforeCommit);
        if let Some((entered, release)) = pause_after_effect {
            entered.notify_one();
            release.notified().await;
        }
        self.set_phase(CommitPhase::CommitInvoked);
        if transaction.commit().await.is_err() {
            return Err(Error::CommitOutcomeUnknown);
        }
        self.set_phase(CommitPhase::DefiniteResult);
        self.return_if_clean(connection).await;
        Ok(outcome)
    }

    #[cfg(test)]
    pub(crate) async fn write_probe_paused(
        &self,
        engine: &Mutex<Option<Database>>,
        operation_id: &str,
        fingerprint: &[u8],
        value: &[u8],
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Result<ProbeWriteOutcome, Error> {
        self.write_probe_inner(
            engine,
            operation_id,
            fingerprint,
            value,
            Some((entered, release)),
        )
        .await
    }

    async fn take_or_connect(&self, engine: &Mutex<Option<Database>>) -> Result<Connection, Error> {
        let retained = self.connection.lock().await.take();
        if let Some(connection) = retained {
            return Ok(connection);
        }
        let guard = engine.lock().await;
        let database = guard.as_ref().ok_or(Error::Shutdown)?;
        let connection = database.connect().map_err(Error::from_connect)?;
        sql::configure_connection(&connection, self.busy_timeout).await?;
        Ok(connection)
    }

    async fn return_if_clean(&self, connection: Connection) {
        if sanitize(&connection).await.is_ok() {
            *self.connection.lock().await = Some(connection);
        }
    }

    pub(crate) async fn close(&self) {
        let _gate = self.gate.lock().await;
        self.connection.lock().await.take();
    }

    #[cfg(test)]
    pub(crate) async fn foreign_keys_enabled(
        &self,
        engine: &Mutex<Option<Database>>,
        recreate: bool,
    ) -> Result<bool, Error> {
        let _gate = self.gate.lock().await;
        if recreate {
            self.connection.lock().await.take();
        }
        let connection = self.take_or_connect(engine).await?;
        let enabled = sql::foreign_keys_enforced(&connection).await?;
        self.return_if_clean(connection).await;
        Ok(enabled)
    }

    #[cfg(test)]
    pub(crate) fn set_semantic_failpoint(&self, failpoint: SemanticFailpoint) {
        self.semantic_failpoint
            .store(failpoint as u8, Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) fn semantic_failpoint(&self) -> SemanticFailpoint {
        match self.semantic_failpoint.load(Ordering::Acquire) {
            1 => SemanticFailpoint::AfterOutcome,
            2 => SemanticFailpoint::AfterRoot,
            3 => SemanticFailpoint::AfterFinish,
            4 => SemanticFailpoint::AfterCommit,
            _ => SemanticFailpoint::Disabled,
        }
    }

    pub(crate) fn phase(&self) -> CommitPhase {
        match self.phase.load(Ordering::Acquire) {
            1 => CommitPhase::DuringEffects,
            2 => CommitPhase::BeforeCommit,
            3 => CommitPhase::CommitInvoked,
            4 => CommitPhase::DefiniteResult,
            _ => CommitPhase::BeforeTransaction,
        }
    }

    fn set_phase(&self, phase: CommitPhase) {
        self.phase.store(phase as u8, Ordering::Release);
    }
}

pub async fn sanitize(connection: &Connection) -> Result<(), Error> {
    connection.execute(sql::SANITIZE, ()).await?;
    connection.execute(sql::ASSERT_CLEAN_BEGIN, ()).await?;
    connection.execute(sql::ASSERT_CLEAN_ROLLBACK, ()).await?;
    Ok(())
}
