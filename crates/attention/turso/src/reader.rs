use crate::BusyTimeout;
use crate::Error;
use crate::decode;
use crate::sql;
use crate::writer::sanitize;
use futures::future::BoxFuture;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use turso_db::Connection;
use turso_db::Database;
use turso_db::params;
use turso_db::transaction::Transaction;
use turso_db::transaction::TransactionBehavior;

#[derive(Debug)]
pub struct ReaderPool {
    connections: Mutex<Vec<Connection>>,
    permits: Arc<Semaphore>,
    busy_timeout: BusyTimeout,
}

impl ReaderPool {
    pub(crate) fn new(connections: Vec<Connection>, busy_timeout: BusyTimeout) -> Arc<Self> {
        let count = connections.len();
        Arc::new(Self {
            connections: Mutex::new(connections),
            permits: Arc::new(Semaphore::new(count)),
            busy_timeout,
        })
    }

    pub(crate) async fn read_probe(
        &self,
        engine: &Mutex<Option<Database>>,
        operation_id: &str,
    ) -> Result<Option<(Vec<u8>, Vec<u8>)>, Error> {
        self.read_probe_inner(engine, operation_id, None).await
    }

    #[cfg(test)]
    pub(crate) async fn read_probe_paused(
        &self,
        engine: &Mutex<Option<Database>>,
        operation_id: &str,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Result<Option<(Vec<u8>, Vec<u8>)>, Error> {
        self.read_probe_inner(engine, operation_id, Some((entered, release)))
            .await
    }

    async fn read_probe_inner(
        &self,
        engine: &Mutex<Option<Database>>,
        operation_id: &str,
        pause_before_rollback: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    ) -> Result<Option<(Vec<u8>, Vec<u8>)>, Error> {
        let _permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| Error::Shutdown)?;
        let mut connection = self.take_or_connect(engine).await?;
        let result = self
            .read_probe_with_connection(&mut connection, operation_id, pause_before_rollback)
            .await;
        if sanitize(&connection).await.is_ok() {
            self.connections.lock().await.push(connection);
        }
        result
    }

    pub(crate) async fn with_snapshot<T, F>(
        &self,
        engine: &Mutex<Option<Database>>,
        operation: F,
    ) -> Result<T, Error>
    where
        F: for<'transaction, 'connection> Fn(
            &'transaction Transaction<'connection>,
        ) -> BoxFuture<'transaction, Result<T, Error>>,
    {
        let _permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| Error::Shutdown)?;
        let mut connection = self.take_or_connect(engine).await?;
        let mut final_result = None;
        for delay_ms in [0, 10, 50] {
            if delay_ms != 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            let transaction = match connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
            {
                Ok(transaction) => transaction,
                Err(error) => {
                    let error = Error::from(error);
                    if matches!(error, Error::BusySnapshot(_)) {
                        final_result = Some(Err(error));
                        continue;
                    }
                    final_result = Some(Err(error));
                    break;
                }
            };
            let result = operation(&transaction).await;
            let rollback = transaction.rollback().await.map_err(Error::from);
            if let Err(error) = rollback {
                final_result = Some(Err(error));
                break;
            }
            match result {
                Ok(value) => {
                    final_result = Some(Ok(value));
                    break;
                }
                Err(error @ Error::BusySnapshot(_)) => {
                    final_result = Some(Err(error));
                }
                Err(error) => {
                    final_result = Some(Err(error));
                    break;
                }
            }
        }
        if sanitize(&connection).await.is_ok() {
            self.connections.lock().await.push(connection);
        }
        final_result.ok_or(Error::BusySnapshot(Box::new(std::io::Error::other(
            "snapshot attempts exhausted",
        ))))?
    }

    async fn read_probe_with_connection(
        &self,
        connection: &mut Connection,
        operation_id: &str,
        pause_before_rollback: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    ) -> Result<Option<(Vec<u8>, Vec<u8>)>, Error> {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await?;
        let mut rows = transaction
            .query(sql::SELECT_QUALIFICATION_PROBE, params![operation_id])
            .await?;
        let value = if let Some(row) = rows.next().await? {
            Some((decode::blob(&row, 0)?, decode::blob(&row, 1)?))
        } else {
            None
        };
        drop(rows);
        if let Some((entered, release)) = pause_before_rollback {
            entered.notify_one();
            release.notified().await;
        }
        transaction.rollback().await?;
        Ok(value)
    }

    async fn take_or_connect(&self, engine: &Mutex<Option<Database>>) -> Result<Connection, Error> {
        let retained = self.connections.lock().await.pop();
        if let Some(connection) = retained {
            return Ok(connection);
        }
        let guard = engine.lock().await;
        let database = guard.as_ref().ok_or(Error::Shutdown)?;
        let connection = database.connect().map_err(Error::from_connect)?;
        sql::configure_connection(&connection, self.busy_timeout).await?;
        Ok(connection)
    }

    pub(crate) async fn close(&self) {
        self.permits.close();
        self.connections.lock().await.clear();
    }

    #[cfg(test)]
    pub(crate) async fn foreign_keys_enabled(
        &self,
        engine: &Mutex<Option<Database>>,
        recreate: bool,
    ) -> Result<bool, Error> {
        let _permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| Error::Shutdown)?;
        if recreate {
            self.connections.lock().await.clear();
        }
        let connection = self.take_or_connect(engine).await?;
        let enabled = sql::foreign_keys_enforced(&connection).await?;
        if sanitize(&connection).await.is_ok() {
            self.connections.lock().await.push(connection);
        }
        Ok(enabled)
    }
}
