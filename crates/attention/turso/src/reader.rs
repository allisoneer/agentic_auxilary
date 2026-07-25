use crate::BusyTimeout;
use crate::Error;
use crate::decode;
use crate::sql;
use crate::writer::sanitize;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use turso_db::Connection;
use turso_db::Database;
use turso_db::params;
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
        let _permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| Error::Shutdown)?;
        let mut connection = self.take_or_connect(engine).await?;
        let result = self
            .read_probe_with_connection(&mut connection, operation_id)
            .await;
        if sanitize(&connection).await.is_ok() {
            self.connections.lock().await.push(connection);
        }
        result
    }

    #[cfg(test)]
    pub(crate) async fn read_probe_paused(
        &self,
        engine: &Mutex<Option<Database>>,
        operation_id: &str,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Result<Option<(Vec<u8>, Vec<u8>)>, Error> {
        let _permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| Error::Shutdown)?;
        let mut connection = self.take_or_connect(engine).await?;
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
        entered.notify_one();
        release.notified().await;
        transaction.rollback().await?;
        if sanitize(&connection).await.is_ok() {
            self.connections.lock().await.push(connection);
        }
        Ok(value)
    }

    async fn read_probe_with_connection(
        &self,
        connection: &mut Connection,
        operation_id: &str,
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
        connection
            .busy_timeout(self.busy_timeout.duration())
            .map_err(Error::from)?;
        Ok(connection)
    }

    pub(crate) async fn close(&self) {
        let permits = self.permits.available_permits();
        if permits > 0 {
            self.permits.forget_permits(permits);
        }
        self.permits.close();
        self.connections.lock().await.clear();
    }
}
