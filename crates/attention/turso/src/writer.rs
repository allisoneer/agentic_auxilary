use crate::BusyTimeout;
use crate::Error;
use crate::decode;
use crate::migration;
use crate::migration::MigrationReport;
use crate::sql;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;
use tokio::sync::Mutex;
use turso_db::Connection;
use turso_db::Database;
use turso_db::params;
use turso_db::transaction::TransactionBehavior;

#[derive(Debug)]
pub struct Writer {
    connection: Mutex<Option<Connection>>,
    gate: Mutex<()>,
    busy_timeout: BusyTimeout,
    phase: AtomicU8,
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
    pub(crate) fn new(connection: Connection, busy_timeout: BusyTimeout) -> Arc<Self> {
        Arc::new(Self {
            connection: Mutex::new(Some(connection)),
            gate: Mutex::new(()),
            busy_timeout,
            phase: AtomicU8::new(CommitPhase::BeforeTransaction as u8),
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
            let mut rows = transaction
                .query(sql::SELECT_QUALIFICATION_PROBE, params![operation_id])
                .await?;
            let row = rows.next().await?.ok_or(Error::ProbeIdentityConflict)?;
            let stored_fingerprint = decode::blob(&row, 0)?;
            let stored_value = decode::blob(&row, 1)?;
            if stored_fingerprint != fingerprint || stored_value != value {
                drop(rows);
                let _ = transaction.rollback().await;
                self.return_if_clean(connection).await;
                return Err(Error::ProbeIdentityConflict);
            }
            drop(rows);
            ProbeWriteOutcome::Replayed
        } else {
            ProbeWriteOutcome::Applied
        };
        if let Some((entered, release)) = pause_after_effect {
            entered.notify_one();
            release.notified().await;
        }
        self.set_phase(CommitPhase::BeforeCommit);
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
        connection
            .busy_timeout(self.busy_timeout.duration())
            .map_err(Error::from)?;
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
