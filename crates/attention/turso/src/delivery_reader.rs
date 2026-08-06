use crate::Error;
use crate::domain_sql;
use crate::mapping;
use crate::reader::ReaderPool;
use attention_kernel::CheckpointQuery;
use attention_kernel::CommitCursor;
use attention_kernel::DeliveryAuthority;
use attention_kernel::DeliveryCheckpoint;
use attention_kernel::DeliveryStatus;
use attention_kernel::OutboxIntentId;
use futures::FutureExt;
use tokio::sync::Mutex;
use turso_db::Database;
use turso_db::params;

pub async fn inspect(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    intent_id: OutboxIntentId,
) -> Result<Option<DeliveryAuthority>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            async move {
                let mut rows = transaction
                    .query(
                        domain_sql::SELECT_DELIVERY_AUTHORITY,
                        params![mapping::id(intent_id)],
                    )
                    .await?;
                let authority = rows
                    .next()
                    .await?
                    .map(|row| mapping::delivery_authority(&row))
                    .transpose()?;
                drop(rows);
                Ok(authority)
            }
            .boxed()
        })
        .await
}

pub async fn status(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    intent_id: OutboxIntentId,
) -> Result<Option<DeliveryStatus>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            async move {
                let mut rows = transaction
                    .query(
                        domain_sql::SELECT_DELIVERY_STATE,
                        params![mapping::id(intent_id)],
                    )
                    .await?;
                let status = rows
                    .next()
                    .await?
                    .map(|row| mapping::delivery_status(&row, 0))
                    .transpose()?;
                drop(rows);
                Ok(status)
            }
            .boxed()
        })
        .await
}

pub async fn checkpoint(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    query: CheckpointQuery,
) -> Result<Option<DeliveryCheckpoint>, Error> {
    let worker = mapping::delivery_text(query.worker())?.to_owned();
    readers
        .with_snapshot(engine, move |transaction| {
            let worker = worker.clone();
            async move {
                let mut rows = transaction
                    .query(domain_sql::SELECT_DELIVERY_CHECKPOINT, params![worker])
                    .await?;
                let checkpoint = rows
                    .next()
                    .await?
                    .map(|row| mapping::delivery_checkpoint(&row))
                    .transpose()?;
                drop(rows);
                Ok(checkpoint)
            }
            .boxed()
        })
        .await
}

pub async fn checkpoint_cursor(
    readers: &ReaderPool,
    engine: &Mutex<Option<Database>>,
    worker: String,
) -> Result<Option<CommitCursor>, Error> {
    readers
        .with_snapshot(engine, move |transaction| {
            let worker = worker.clone();
            async move {
                let mut rows = transaction
                    .query(domain_sql::SELECT_DELIVERY_CHECKPOINT, params![worker])
                    .await?;
                let cursor = match rows.next().await? {
                    Some(row) => Some(mapping::parse_checkpoint_cursor(&crate::decode::blob(
                        &row, 1,
                    )?)?),
                    None => None,
                };
                drop(rows);
                Ok(cursor)
            }
            .boxed()
        })
        .await
}
