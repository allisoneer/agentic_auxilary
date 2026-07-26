use crate::Config;
use crate::Error;
use crate::LifecycleState;
use crate::backup;
use crate::backup::BackupManifest;
use crate::lifecycle::Lifecycle;
use crate::migration;
use crate::migration::MigrationReport;
use crate::path::DirectoryOwnership;
use crate::reader::ReaderPool;
use crate::writer::CommitPhase;
use crate::writer::ProbeWriteOutcome;
use crate::writer::Writer;
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
    writer_connection
        .busy_timeout(config.busy_timeout().duration())
        .map_err(Error::from)?;
    migration::preflight(&writer_connection).await?;
    let writer = Writer::new(writer_connection, config.busy_timeout());
    let mut reader_connections = Vec::with_capacity(config.reader_count());
    for _ in 0..config.reader_count() {
        let connection = engine.connect().map_err(Error::from_connect)?;
        connection
            .busy_timeout(config.busy_timeout().duration())
            .map_err(Error::from)?;
        reader_connections.push(connection);
    }
    let readers = ReaderPool::new(reader_connections, config.busy_timeout());
    Ok((engine, writer, readers))
}

#[cfg(test)]
mod tests {
    use super::*;
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
