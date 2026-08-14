use crate::Config;
use crate::Error;
use crate::LifecycleState;
use crate::backup;
use crate::backup::BackupManifest;
use crate::delivery_reader;
use crate::delivery_writer;
use crate::identity;
use crate::identity::PersistentServerIdentity;
use crate::lifecycle::Lifecycle;
use crate::migration;
use crate::migration::MigrationReport;
use crate::path::DirectoryOwnership;
use crate::reader::ReaderPool;
use crate::semantic_reader;
use crate::semantic_writer;
use crate::sql;
use crate::writer::CommitPhase;
use crate::writer::ProbeWriteOutcome;
use crate::writer::Writer;
use attention_kernel::AcknowledgeAttentionSignalBundle;
use attention_kernel::AcknowledgeAttentionSignalResult;
use attention_kernel::AcknowledgeReminderFireBundle;
use attention_kernel::AcknowledgeReminderFireResult;
use attention_kernel::AttentionSignal;
use attention_kernel::AttentionSignalId;
use attention_kernel::AttentionSnapshot;
use attention_kernel::BoundedDeliveryText;
use attention_kernel::CancelWorkItemBundle;
use attention_kernel::CancelWorkItemResult;
use attention_kernel::ChangeEvent;
use attention_kernel::ChangeEventId;
use attention_kernel::ChangesAfterQuery;
use attention_kernel::ChangesResult;
use attention_kernel::CheckpointAdvance;
use attention_kernel::CheckpointAdvanceOutcome;
use attention_kernel::CheckpointQuery;
use attention_kernel::CompleteWorkItemBundle;
use attention_kernel::CompleteWorkItemResult;
use attention_kernel::CreateReminderBundle;
use attention_kernel::CreateReminderResult;
use attention_kernel::CreateWorkItemBundle;
use attention_kernel::CreateWorkItemResult;
use attention_kernel::DeliveryAuthority;
use attention_kernel::DeliveryCheckpoint;
use attention_kernel::DeliveryClaim;
use attention_kernel::DeliveryClaimQuery;
use attention_kernel::DeliveryCompletionOutcome;
use attention_kernel::DeliveryLeaseToken;
use attention_kernel::DueReminderFire;
use attention_kernel::DueReminderFiresQuery;
use attention_kernel::FireReminderBundle;
use attention_kernel::FireReminderResult;
use attention_kernel::IngestSourceOccurrenceBundle;
use attention_kernel::IngestSourceOccurrenceResult;
use attention_kernel::MutationIdempotencyKey;
use attention_kernel::OutboxIntentId;
use attention_kernel::PortError;
use attention_kernel::PriorMutationOutcome;
use attention_kernel::PriorMutationRecord;
use attention_kernel::ProviderMessageId;
use attention_kernel::Reminder;
use attention_kernel::ReminderId;
use attention_kernel::RenewOutcome;
use attention_kernel::SnoozeReminderFireBundle;
use attention_kernel::SnoozeReminderFireResult;
use attention_kernel::SourceAuthorityQuery;
use attention_kernel::SourceEntity;
use attention_kernel::SourceReceipt;
use attention_kernel::SourceReceiptId;
use attention_kernel::WorkItem;
use attention_kernel::WorkItemId;
use chrono::DateTime;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Mutex;
use turso_db::Builder;
use turso_db::Database;

#[derive(Debug, Clone)]
pub struct AttentionDatabase {
    inner: Arc<Inner>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeResolution {
    Matching(Vec<u8>),
    DefinitelyAbsent,
    IdentityConflict,
}

#[derive(Debug)]
struct Inner {
    config: Config,
    lifecycle: Arc<Lifecycle>,
    engine: Mutex<Option<Database>>,
    writer: Mutex<Option<Arc<Writer>>>,
    readers: Mutex<Option<Arc<ReaderPool>>>,
    ownership: Mutex<Option<DirectoryOwnership>>,
}

impl AttentionDatabase {
    async fn semantic_readers(&self) -> Result<Arc<ReaderPool>, Error> {
        self.inner
            .readers
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)
    }

    pub(crate) async fn semantic_work_item(
        &self,
        id: WorkItemId,
    ) -> Result<Option<WorkItem>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::work_item(&readers, &self.inner.engine, id).await
    }

    pub(crate) async fn semantic_signal(
        &self,
        id: AttentionSignalId,
    ) -> Result<Option<AttentionSignal>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::signal(&readers, &self.inner.engine, id).await
    }

    pub(crate) async fn semantic_reminder(
        &self,
        id: ReminderId,
    ) -> Result<Option<Reminder>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::reminder(&readers, &self.inner.engine, id).await
    }

    pub(crate) async fn semantic_entity(
        &self,
        query: &SourceAuthorityQuery,
    ) -> Result<Option<SourceEntity>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::entity(&readers, &self.inner.engine, query).await
    }

    pub(crate) async fn semantic_receipt(
        &self,
        id: SourceReceiptId,
    ) -> Result<Option<SourceReceipt>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::receipt(&readers, &self.inner.engine, id).await
    }

    pub(crate) async fn semantic_outcome(
        &self,
        key: MutationIdempotencyKey,
    ) -> Result<Option<PriorMutationOutcome>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::outcome(&readers, &self.inner.engine, key).await
    }

    pub(crate) async fn semantic_prior_mutation(
        &self,
        key: MutationIdempotencyKey,
    ) -> Result<Option<PriorMutationRecord>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::prior_mutation(&readers, &self.inner.engine, key).await
    }

    pub(crate) async fn semantic_change_event(
        &self,
        id: ChangeEventId,
    ) -> Result<Option<ChangeEvent>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::change_event(&readers, &self.inner.engine, id).await
    }

    pub(crate) async fn semantic_snapshot(&self) -> Result<AttentionSnapshot, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::snapshot(&readers, &self.inner.engine).await
    }

    pub(crate) async fn semantic_changes(
        &self,
        query: ChangesAfterQuery,
    ) -> Result<ChangesResult, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::changes(&readers, &self.inner.engine, query).await
    }

    pub(crate) async fn semantic_due_reminder_fires(
        &self,
        query: DueReminderFiresQuery,
    ) -> Result<Vec<DueReminderFire>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        semantic_reader::due_reminder_fires(&readers, &self.inner.engine, query).await
    }

    pub(crate) async fn delivery_inspect(
        &self,
        intent_id: OutboxIntentId,
    ) -> Result<Option<DeliveryAuthority>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        delivery_reader::inspect(&readers, &self.inner.engine, intent_id).await
    }

    pub(crate) async fn delivery_claim(
        &self,
        query: DeliveryClaimQuery,
    ) -> Result<Vec<DeliveryClaim>, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, _readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        delivery_writer::claim(&writer, &self.inner.engine, query).await
    }

    pub(crate) async fn delivery_renew(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        expires_at: DateTime<Utc>,
    ) -> Result<RenewOutcome, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        delivery_writer::renew(
            &writer,
            &readers,
            &self.inner.engine,
            intent_id,
            token,
            expires_at,
        )
        .await
    }

    pub(crate) async fn delivery_succeed(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        provider_message_id: ProviderMessageId,
        succeeded_at: DateTime<Utc>,
    ) -> Result<DeliveryCompletionOutcome, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        delivery_writer::succeed(
            &writer,
            &readers,
            &self.inner.engine,
            intent_id,
            token,
            provider_message_id,
            succeeded_at,
        )
        .await
    }

    pub(crate) async fn delivery_fail_retryable(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        attempt: u32,
        error: BoundedDeliveryText,
        next_retry_at: DateTime<Utc>,
    ) -> Result<DeliveryCompletionOutcome, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        delivery_writer::fail_retryable(
            &writer,
            &readers,
            &self.inner.engine,
            intent_id,
            token,
            (attempt, error, next_retry_at),
        )
        .await
    }

    pub(crate) async fn delivery_fail_terminal(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        attempt: u32,
        error: BoundedDeliveryText,
        failed_at: DateTime<Utc>,
    ) -> Result<DeliveryCompletionOutcome, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        delivery_writer::fail_terminal(
            &writer,
            &readers,
            &self.inner.engine,
            intent_id,
            token,
            (attempt, error, failed_at),
        )
        .await
    }

    pub(crate) async fn delivery_skip(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        reason: BoundedDeliveryText,
        skipped_at: DateTime<Utc>,
    ) -> Result<DeliveryCompletionOutcome, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        delivery_writer::skip(
            &writer,
            &readers,
            &self.inner.engine,
            intent_id,
            token,
            reason,
            skipped_at,
        )
        .await
    }

    pub(crate) async fn delivery_checkpoint(
        &self,
        query: CheckpointQuery,
    ) -> Result<Option<DeliveryCheckpoint>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self.semantic_readers().await?;
        delivery_reader::checkpoint(&readers, &self.inner.engine, query).await
    }

    pub(crate) async fn delivery_advance_checkpoint(
        &self,
        advance: CheckpointAdvance,
    ) -> Result<CheckpointAdvanceOutcome, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        delivery_writer::advance_checkpoint(&writer, &readers, &self.inner.engine, advance).await
    }

    async fn semantic_writer_parts(&self) -> Result<(Arc<Writer>, Arc<ReaderPool>), Error> {
        let writer = self
            .inner
            .writer
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        Ok((writer, self.semantic_readers().await?))
    }

    pub(crate) async fn semantic_create_work_item(
        &self,
        bundle: CreateWorkItemBundle,
    ) -> Result<CreateWorkItemResult, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        semantic_writer::create_work_item(&writer, &readers, &self.inner.engine, bundle).await
    }

    pub(crate) async fn semantic_complete_work_item(
        &self,
        bundle: CompleteWorkItemBundle,
    ) -> Result<CompleteWorkItemResult, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        semantic_writer::complete_work_item(&writer, &readers, &self.inner.engine, bundle).await
    }

    pub(crate) async fn semantic_cancel_work_item(
        &self,
        bundle: CancelWorkItemBundle,
    ) -> Result<CancelWorkItemResult, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        semantic_writer::cancel_work_item(&writer, &readers, &self.inner.engine, bundle).await
    }

    pub(crate) async fn semantic_acknowledge_signal(
        &self,
        bundle: AcknowledgeAttentionSignalBundle,
    ) -> Result<AcknowledgeAttentionSignalResult, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        semantic_writer::acknowledge_signal(&writer, &readers, &self.inner.engine, bundle).await
    }

    pub(crate) async fn semantic_ingest_source(
        &self,
        bundle: IngestSourceOccurrenceBundle,
    ) -> Result<IngestSourceOccurrenceResult, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        semantic_writer::ingest_source(&writer, &readers, &self.inner.engine, bundle).await
    }

    pub(crate) async fn semantic_create_reminder(
        &self,
        bundle: CreateReminderBundle,
    ) -> Result<CreateReminderResult, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        semantic_writer::create_reminder(&writer, &readers, &self.inner.engine, bundle).await
    }

    pub(crate) async fn semantic_fire_reminder(
        &self,
        bundle: FireReminderBundle,
    ) -> Result<FireReminderResult, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        semantic_writer::fire_reminder(&writer, &readers, &self.inner.engine, bundle).await
    }

    pub(crate) async fn semantic_acknowledge_reminder(
        &self,
        bundle: AcknowledgeReminderFireBundle,
    ) -> Result<AcknowledgeReminderFireResult, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        semantic_writer::acknowledge_reminder(&writer, &readers, &self.inner.engine, bundle).await
    }

    pub(crate) async fn semantic_snooze_reminder(
        &self,
        bundle: SnoozeReminderFireBundle,
    ) -> Result<SnoozeReminderFireResult, PortError<Error>> {
        let _lifecycle = self.inner.lifecycle.acquire().map_err(PortError::Adapter)?;
        let (writer, readers) = self
            .semantic_writer_parts()
            .await
            .map_err(PortError::Adapter)?;
        semantic_writer::snooze_reminder(&writer, &readers, &self.inner.engine, bundle).await
    }

    pub async fn open(config: Config) -> Result<Self, Error> {
        let ownership = config
            .database_directory()
            .acquire()
            .map_err(|error| Error::Ownership(Box::new(error)))?;
        let (engine, writer, readers) = open_engine(&config).await?;
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                lifecycle: Lifecycle::open(),
                engine: Mutex::new(Some(engine)),
                writer: Mutex::new(Some(writer)),
                readers: Mutex::new(Some(readers)),
                ownership: Mutex::new(Some(ownership)),
            }),
        })
    }

    pub fn state(&self) -> LifecycleState {
        self.inner.lifecycle.state()
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    pub async fn write_qualification_probe(
        &self,
        operation_id: &str,
        fingerprint: &[u8],
        value: &[u8],
    ) -> Result<ProbeWriteOutcome, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let writer = self
            .inner
            .writer
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        writer
            .write_probe(&self.inner.engine, operation_id, fingerprint, value)
            .await
    }

    pub async fn resolve_qualification_probe(
        &self,
        operation_id: &str,
        expected_fingerprint: &[u8],
    ) -> Result<ProbeResolution, Error> {
        match self.read_qualification_probe(operation_id).await? {
            Some((fingerprint, value)) if fingerprint == expected_fingerprint => {
                Ok(ProbeResolution::Matching(value))
            }
            Some(_) => Ok(ProbeResolution::IdentityConflict),
            None => Ok(ProbeResolution::DefinitelyAbsent),
        }
    }

    pub async fn last_commit_phase(&self) -> Result<CommitPhase, Error> {
        let writer = self
            .inner
            .writer
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        Ok(writer.phase())
    }

    pub async fn read_qualification_probe(
        &self,
        operation_id: &str,
    ) -> Result<Option<(Vec<u8>, Vec<u8>)>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self
            .inner
            .readers
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        readers.read_probe(&self.inner.engine, operation_id).await
    }

    /// Atomically initializes the singleton persistent server and stream identity, or returns the
    /// identity already stored by an earlier caller or process boot.
    pub async fn load_or_create_server_identity(
        &self,
        candidate: PersistentServerIdentity,
    ) -> Result<PersistentServerIdentity, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let (writer, _) = self.semantic_writer_parts().await?;
        identity::load_or_create(&writer, &self.inner.engine, candidate).await
    }

    /// Apply bundled forward migrations from the sole startup composition point.
    pub async fn run_startup_migrations(&self) -> Result<MigrationReport, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let writer = self
            .inner
            .writer
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        writer.run_migrations(&self.inner.engine).await
    }

    pub async fn close(&self) -> Result<(), Error> {
        self.inner.lifecycle.begin_drain().await?;
        let writer = self.inner.writer.lock().await.take();
        if let Some(writer) = writer {
            writer.close().await;
        }
        let readers = self.inner.readers.lock().await.take();
        if let Some(readers) = readers {
            readers.close().await;
        }
        self.inner.engine.lock().await.take();
        self.inner.ownership.lock().await.take();
        self.inner.lifecycle.finish_close();
        Ok(())
    }

    pub fn backup(&self, name: &str) -> Result<BackupManifest, Error> {
        if self.state() != LifecycleState::Closed {
            return Err(Error::Lifecycle);
        }
        backup::create(&self.inner.config, name)
    }

    pub async fn restore(config: Config, name: &str) -> Result<Self, Error> {
        backup::restore_files(&config, name)?;
        Self::open(config).await
    }

    pub async fn reopen(&self) -> Result<(), Error> {
        self.inner.lifecycle.begin_open()?;
        let result = self.reopen_inner().await;
        self.inner.lifecycle.finish_open(result.is_ok());
        result
    }

    async fn reopen_inner(&self) -> Result<(), Error> {
        let ownership = self
            .inner
            .config
            .database_directory()
            .acquire()
            .map_err(|error| Error::Ownership(Box::new(error)))?;
        let (engine, writer, readers) = open_engine(&self.inner.config).await?;
        *self.inner.engine.lock().await = Some(engine);
        *self.inner.writer.lock().await = Some(writer);
        *self.inner.readers.lock().await = Some(readers);
        *self.inner.ownership.lock().await = Some(ownership);
        Ok(())
    }

    #[cfg(test)]
    async fn read_qualification_probe_paused(
        &self,
        operation_id: &str,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Result<Option<(Vec<u8>, Vec<u8>)>, Error> {
        let _lifecycle = self.inner.lifecycle.acquire()?;
        let readers = self
            .inner
            .readers
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        readers
            .read_probe_paused(&self.inner.engine, operation_id, entered, release)
            .await
    }
}

async fn open_engine(config: &Config) -> Result<(Database, Arc<Writer>, Arc<ReaderPool>), Error> {
    let path = config
        .database_directory()
        .database_file()
        .to_str()
        .ok_or(crate::PathError::NonUtf8)?
        .to_owned();
    let engine = Builder::new_local(&path)
        .build()
        .await
        .map_err(Error::from_open)?;
    let writer_connection = engine.connect().map_err(Error::from_connect)?;
    sql::configure_connection(&writer_connection, config.busy_timeout()).await?;
    migration::preflight(&writer_connection).await?;
    let writer = Writer::new(writer_connection, config.busy_timeout());
    let mut reader_connections = Vec::with_capacity(config.reader_count());
    for _ in 0..config.reader_count() {
        let connection = engine.connect().map_err(Error::from_connect)?;
        sql::configure_connection(&connection, config.busy_timeout()).await?;
        reader_connections.push(connection);
    }
    let readers = ReaderPool::new(reader_connections, config.busy_timeout());
    Ok((engine, writer, readers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::SemanticFailpoint;
    use attention_kernel::AttentionCommitPort;
    use attention_kernel::AttentionReadPort;
    use attention_kernel::BoundedDeliveryText;
    use attention_kernel::ChangeEventId;
    use attention_kernel::CheckpointAdvance;
    use attention_kernel::CheckpointAdvanceOutcome;
    use attention_kernel::ClaimLimit;
    use attention_kernel::CommitCursor;
    use attention_kernel::CreateReminder;
    use attention_kernel::CreateWorkItem;
    use attention_kernel::DeliveryClaimQuery;
    use attention_kernel::DeliveryCompletionOutcome;
    use attention_kernel::DeliveryStatus;
    use attention_kernel::EvaluationContext;
    use attention_kernel::FireReminder;
    use attention_kernel::MutationIdempotencyKey;
    use attention_kernel::OutboxIntentId;
    use attention_kernel::PriorOutcomeQuery;
    use attention_kernel::ProviderMessageId;
    use attention_kernel::ReminderFireId;
    use attention_kernel::ReminderFireState;
    use attention_kernel::ReminderId;
    use attention_kernel::ReminderTarget;
    use attention_kernel::WorkItemId;
    use attention_kernel::evaluate_create_reminder;
    use attention_kernel::evaluate_create_work_item;
    use attention_kernel::evaluate_fire_reminder;
    use chrono::Utc;
    use futures::FutureExt;
    use std::time::Duration;
    use tokio::sync::Notify;
    use turso_db::params;

    async fn database(
        reader_count: usize,
    ) -> Result<(tempfile::TempDir, AttentionDatabase), Error> {
        let root = tempfile::tempdir().map_err(Error::Io)?;
        let config = Config::new(root.path().join("database"), root.path().join("backups"))?
            .with_reader_count(reader_count)?;
        let database = AttentionDatabase::open(config).await?;
        database.run_startup_migrations().await?;
        Ok((root, database))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn writer_is_serial_and_dropped_future_is_quarantined() -> Result<(), Error> {
        let (_root, database) = database(2).await?;
        let writer = database
            .inner
            .writer
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let first_database = database.clone();
        let first_writer = Arc::clone(&writer);
        let first_entered = Arc::clone(&entered);
        let first_release = Arc::clone(&release);
        let first = tokio::spawn(async move {
            first_writer
                .write_probe_paused(
                    &first_database.inner.engine,
                    "first",
                    b"fingerprint",
                    b"value",
                    first_entered,
                    first_release,
                )
                .await
        });
        entered.notified().await;
        assert_eq!(writer.phase(), CommitPhase::BeforeCommit);
        let second_database = database.clone();
        let second = tokio::spawn(async move {
            second_database
                .write_qualification_probe("second", b"fingerprint", b"value")
                .await
        });
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(
            !second.is_finished(),
            "second writer entered while first held gate"
        );
        first.abort();
        let _ = first.await;
        second
            .await
            .map_err(|error| Error::Engine(Box::new(error)))??;
        assert!(database.read_qualification_probe("first").await?.is_none());
        assert!(database.read_qualification_probe("second").await?.is_some());
        database.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn replay_decode_failure_preserves_error_and_writer_progress() -> Result<(), Error> {
        let (_root, database) = database(1).await?;
        let malformed = database
            .inner
            .engine
            .lock()
            .await
            .as_ref()
            .ok_or(Error::Shutdown)?
            .connect()
            .map_err(Error::from_connect)?;
        malformed
            .execute(
                "INSERT INTO __attention_probe (operation_id, fingerprint, value) VALUES (?1, ?2, ?3)",
                params!["malformed", "not-a-blob", b"value".to_vec()],
            )
            .await?;
        drop(malformed);

        assert!(matches!(
            database
                .write_qualification_probe("malformed", b"fingerprint", b"value")
                .await,
            Err(Error::Decode(_))
        ));
        assert_eq!(
            database
                .write_qualification_probe("after-decode", b"fingerprint", b"value")
                .await?,
            ProbeWriteOutcome::Applied
        );

        database.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn foreign_keys_are_enabled_on_initial_and_recreated_connections() -> Result<(), Error> {
        let (_root, database) = database(2).await?;
        let writer = database
            .inner
            .writer
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        let readers = database
            .inner
            .readers
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        assert!(
            writer
                .foreign_keys_enabled(&database.inner.engine, false)
                .await?
        );
        assert!(
            writer
                .foreign_keys_enabled(&database.inner.engine, true)
                .await?
        );
        assert!(
            readers
                .foreign_keys_enabled(&database.inner.engine, false)
                .await?
        );
        assert!(
            readers
                .foreign_keys_enabled(&database.inner.engine, true)
                .await?
        );
        database.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn semantic_failpoints_roll_back_outcome_root_event_and_head() -> Result<(), Error> {
        for failpoint in [
            SemanticFailpoint::AfterOutcome,
            SemanticFailpoint::AfterRoot,
            SemanticFailpoint::AfterFinish,
        ] {
            let (_root, database) = database(1).await?;
            let command = CreateWorkItem::new(
                WorkItemId::new(),
                None,
                None,
                None,
                None,
                MutationIdempotencyKey::new(),
            );
            let bundle = evaluate_create_work_item(
                &command,
                EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
            );
            let writer = database
                .inner
                .writer
                .lock()
                .await
                .as_ref()
                .cloned()
                .ok_or(Error::Shutdown)?;
            writer.set_semantic_failpoint(failpoint);
            assert!(
                database
                    .commit_create_work_item(bundle.clone())
                    .await
                    .is_err()
            );
            writer.set_semantic_failpoint(SemanticFailpoint::Disabled);
            assert!(
                database
                    .work_item(command.id())
                    .await
                    .is_ok_and(|row| row.is_none())
            );
            assert!(
                database
                    .prior_outcome(PriorOutcomeQuery::new(command.idempotency_key()))
                    .await
                    .is_ok_and(|row| row.is_none())
            );
            assert_eq!(
                database
                    .snapshot()
                    .await
                    .map_err(|error| match error {
                        PortError::Adapter(error) => error,
                        PortError::Semantic(_) => Error::MigrationIntegrity(
                            "snapshot unexpectedly returned a semantic error",
                        ),
                    })?
                    .cursor(),
                CommitCursor::try_from(1).map_err(|error| Error::Decode(Box::new(error)))?
            );
            database
                .commit_create_work_item(bundle)
                .await
                .map_err(|error| match error {
                    PortError::Adapter(error) => error,
                    PortError::Semantic(_) => {
                        Error::MigrationIntegrity("retry unexpectedly returned a semantic error")
                    }
                })?;
            database.close().await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn post_finish_failpoint_rolls_back_outbox_and_pending_authority() -> Result<(), Error> {
        let (_root, database) = database(1).await?;
        let reminder_id = ReminderId::new();
        let fire_id = ReminderFireId::new();
        database
            .commit_create_reminder(evaluate_create_reminder(
                &CreateReminder::new(
                    reminder_id,
                    fire_id,
                    ReminderTarget::WorkItem(WorkItemId::new()),
                    Utc::now(),
                    MutationIdempotencyKey::new(),
                ),
                EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
            ))
            .await
            .map_err(|error| match error {
                PortError::Adapter(error) => error,
                PortError::Semantic(_) => {
                    Error::MigrationIntegrity("reminder setup returned semantic error")
                }
            })?;
        let reminder = database
            .reminder(reminder_id)
            .await
            .map_err(|error| match error {
                PortError::Adapter(error) => error,
                PortError::Semantic(_) => {
                    Error::MigrationIntegrity("reminder read returned semantic error")
                }
            })?
            .ok_or(Error::MigrationIntegrity("reminder setup is missing"))?;
        let intent_id = OutboxIntentId::new();
        let bundle = evaluate_fire_reminder(
            &FireReminder::new(reminder_id, fire_id, MutationIdempotencyKey::new()),
            &reminder,
            EvaluationContext::new(ChangeEventId::new(), Some(intent_id), Utc::now()),
        )
        .map_err(|error| Error::Decode(Box::new(error)))?;
        let writer = database
            .inner
            .writer
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        writer.set_semantic_failpoint(SemanticFailpoint::AfterFinish);
        assert!(database.commit_fire_reminder(bundle.clone()).await.is_err());
        writer.set_semantic_failpoint(SemanticFailpoint::Disabled);

        let reminder = database
            .reminder(reminder_id)
            .await
            .map_err(|error| match error {
                PortError::Adapter(error) => error,
                PortError::Semantic(_) => {
                    Error::MigrationIntegrity("reminder read returned semantic error")
                }
            })?
            .ok_or(Error::MigrationIntegrity("reminder disappeared"))?;
        assert_eq!(reminder.fires()[0].state(), ReminderFireState::Scheduled);
        let connection = database
            .inner
            .engine
            .lock()
            .await
            .as_ref()
            .ok_or(Error::Shutdown)?
            .connect()
            .map_err(Error::from_connect)?;
        let mut rows = connection
            .query(
                "SELECT (SELECT count(*) FROM outbox_intents),
                        (SELECT count(*) FROM delivery_states)",
                (),
            )
            .await?;
        let row = rows.next().await?.ok_or(Error::MigrationIntegrity(
            "delivery inventory count missing",
        ))?;
        assert_eq!(row.get::<i64>(0).map_err(Error::from)?, 0);
        assert_eq!(row.get::<i64>(1).map_err(Error::from)?, 0);
        drop(rows);
        drop(connection);

        database
            .commit_fire_reminder(bundle)
            .await
            .map_err(|error| match error {
                PortError::Adapter(error) => error,
                PortError::Semantic(_) => {
                    Error::MigrationIntegrity("fire retry returned semantic error")
                }
            })?;
        writer.set_semantic_failpoint(SemanticFailpoint::AfterCommit);
        let now = Utc::now();
        assert!(matches!(
            database
                .delivery_claim(DeliveryClaimQuery::new(
                    now,
                    now + chrono::Duration::minutes(1),
                    ClaimLimit::try_from(1).map_err(|error| Error::Decode(Box::new(error)))?,
                ))
                .await,
            Err(PortError::Adapter(Error::CommitOutcomeUnknown))
        ));
        let authority = database
            .delivery_inspect(intent_id)
            .await?
            .ok_or(Error::MigrationIntegrity("ambiguous claim did not commit"))?;
        let DeliveryStatus::Leased { token, .. } = authority.state().status() else {
            return Err(Error::MigrationIntegrity(
                "ambiguous claim did not leave a lease",
            ));
        };
        let token = *token;
        assert_eq!(
            database
                .delivery_succeed(
                    intent_id,
                    token,
                    ProviderMessageId::new("ambiguous-provider", 128)
                        .map_err(|error| Error::Decode(Box::new(error)))?,
                    now,
                )
                .await
                .map_err(|error| match error {
                    PortError::Adapter(error) => error,
                    PortError::Semantic(_) => {
                        Error::MigrationIntegrity("delivery success returned semantic error")
                    }
                })?,
            DeliveryCompletionOutcome::Applied
        );
        assert_eq!(
            database
                .delivery_advance_checkpoint(CheckpointAdvance::new(
                    BoundedDeliveryText::new("ambiguous-worker", 128)
                        .map_err(|error| Error::Decode(Box::new(error)))?,
                    None,
                    CommitCursor::try_from(2).map_err(|error| Error::Decode(Box::new(error)))?,
                    intent_id,
                ))
                .await
                .map_err(|error| match error {
                    PortError::Adapter(error) => error,
                    PortError::Semantic(_) => {
                        Error::MigrationIntegrity("checkpoint advance returned semantic error")
                    }
                })?,
            CheckpointAdvanceOutcome::Advanced
        );
        writer.set_semantic_failpoint(SemanticFailpoint::Disabled);
        database.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn post_commit_ambiguity_resolves_original_identity_from_fresh_reader()
    -> Result<(), Error> {
        let (_root, database) = database(1).await?;
        let command = CreateWorkItem::new(
            WorkItemId::new(),
            None,
            None,
            None,
            None,
            MutationIdempotencyKey::new(),
        );
        let bundle = evaluate_create_work_item(
            &command,
            EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
        );
        let writer = database
            .inner
            .writer
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        writer.set_semantic_failpoint(SemanticFailpoint::AfterCommit);
        let resolved = database
            .commit_create_work_item(bundle.clone())
            .await
            .map_err(|error| match error {
                PortError::Adapter(error) => error,
                PortError::Semantic(_) => Error::MigrationIntegrity(
                    "ambiguous commit unexpectedly returned a semantic error",
                ),
            })?;
        writer.set_semantic_failpoint(SemanticFailpoint::Disabled);
        assert_eq!(resolved, resolved.replayed());
        assert_eq!(
            database
                .commit_create_work_item(bundle)
                .await
                .map_err(|error| match error {
                    PortError::Adapter(error) => error,
                    PortError::Semantic(_) =>
                        Error::MigrationIntegrity("replay unexpectedly returned a semantic error",),
                })?,
            resolved
        );
        assert!(
            database
                .work_item(command.id())
                .await
                .is_ok_and(|row| row.is_some())
        );
        database.close().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn held_semantic_snapshot_excludes_concurrent_commit_on_another_connection()
    -> Result<(), Error> {
        let (_root, database) = database(2).await?;
        let readers = database
            .inner
            .readers
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let held_database = database.clone();
        let held_entered = Arc::clone(&entered);
        let held_release = Arc::clone(&release);
        let held = tokio::spawn(async move {
            readers
                .with_snapshot(&held_database.inner.engine, move |transaction| {
                    let entered = Arc::clone(&held_entered);
                    let release = Arc::clone(&held_release);
                    async move {
                        let mut rows = transaction
                            .query("SELECT count(*) FROM work_items", ())
                            .await?;
                        let before = rows
                            .next()
                            .await?
                            .ok_or(Error::MigrationIntegrity("snapshot count is missing"))?
                            .get::<i64>(0)
                            .map_err(Error::from)?;
                        drop(rows);
                        entered.notify_one();
                        release.notified().await;
                        let mut rows = transaction
                            .query("SELECT count(*) FROM work_items", ())
                            .await?;
                        let after = rows
                            .next()
                            .await?
                            .ok_or(Error::MigrationIntegrity("snapshot count is missing"))?
                            .get::<i64>(0)
                            .map_err(Error::from)?;
                        drop(rows);
                        Ok((before, after))
                    }
                    .boxed()
                })
                .await
        });
        entered.notified().await;
        let command = CreateWorkItem::new(
            WorkItemId::new(),
            None,
            None,
            None,
            None,
            MutationIdempotencyKey::new(),
        );
        database
            .commit_create_work_item(evaluate_create_work_item(
                &command,
                EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
            ))
            .await
            .map_err(|error| match error {
                PortError::Adapter(error) => error,
                PortError::Semantic(_) => {
                    Error::MigrationIntegrity("concurrent create returned semantic error")
                }
            })?;
        release.notify_one();
        assert_eq!(
            held.await
                .map_err(|error| Error::Engine(Box::new(error)))??,
            (0, 0)
        );
        assert_eq!(
            database
                .snapshot()
                .await
                .map_err(|error| match error {
                    PortError::Adapter(error) => error,
                    PortError::Semantic(_) => {
                        Error::MigrationIntegrity("snapshot returned semantic error")
                    }
                })?
                .work_items()
                .len(),
            1
        );
        database.close().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn reader_bound_holds_and_writer_progresses_under_snapshots() -> Result<(), Error> {
        let (_root, database) = database(2).await?;
        database
            .write_qualification_probe("seed", b"fingerprint", b"value")
            .await?;
        let readers = database
            .inner
            .readers
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(Error::Shutdown)?;
        let mut held = Vec::new();
        let mut releases = Vec::new();
        for _ in 0..2 {
            let entered = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());
            let task_database = database.clone();
            let task_readers = Arc::clone(&readers);
            let task_entered = Arc::clone(&entered);
            let task_release = Arc::clone(&release);
            held.push(tokio::spawn(async move {
                task_readers
                    .read_probe_paused(
                        &task_database.inner.engine,
                        "seed",
                        task_entered,
                        task_release,
                    )
                    .await
            }));
            entered.notified().await;
            releases.push(release);
        }
        let third_database = database.clone();
        let third_readers = Arc::clone(&readers);
        let third = tokio::spawn(async move {
            third_readers
                .read_probe(&third_database.inner.engine, "seed")
                .await
        });
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(
            !third.is_finished(),
            "reader bound exceeded configured maximum"
        );

        tokio::time::timeout(
            Duration::from_secs(1),
            database.write_qualification_probe("writer", b"fingerprint", b"value"),
        )
        .await
        .map_err(|error| Error::Engine(Box::new(error)))??;
        for release in releases {
            release.notify_one();
        }
        for task in held {
            task.await
                .map_err(|error| Error::Engine(Box::new(error)))??;
        }
        third
            .await
            .map_err(|error| Error::Engine(Box::new(error)))??;
        database.close().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn dropped_reader_is_recreated_and_drain_waits_for_active_snapshot() -> Result<(), Error>
    {
        let (_root, database) = database(2).await?;
        database
            .write_qualification_probe("seed", b"fingerprint", b"value")
            .await?;

        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let task_database = database.clone();
        let task_entered = Arc::clone(&entered);
        let task_release = Arc::clone(&release);
        let dropped = tokio::spawn(async move {
            task_database
                .read_qualification_probe_paused("seed", task_entered, task_release)
                .await
        });
        entered.notified().await;
        dropped.abort();
        let _ = dropped.await;
        assert!(database.read_qualification_probe("seed").await?.is_some());

        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let task_database = database.clone();
        let task_entered = Arc::clone(&entered);
        let task_release = Arc::clone(&release);
        let active = tokio::spawn(async move {
            task_database
                .read_qualification_probe_paused("seed", task_entered, task_release)
                .await
        });
        entered.notified().await;
        let closing_database = database.clone();
        let closing = tokio::spawn(async move { closing_database.close().await });
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(
            !closing.is_finished(),
            "close did not wait for active snapshot"
        );
        assert!(matches!(
            database.read_qualification_probe("seed").await,
            Err(Error::Shutdown)
        ));
        release.notify_one();
        active
            .await
            .map_err(|error| Error::Engine(Box::new(error)))??;
        closing
            .await
            .map_err(|error| Error::Engine(Box::new(error)))??;
        assert_eq!(database.state(), LifecycleState::Closed);
        Ok(())
    }
}
