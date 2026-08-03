use crate::BusyTimeout;
use crate::Error;
use turso_db::Connection;

pub const SANITIZE: &str = "SELECT 1";
pub const ASSERT_CLEAN_BEGIN: &str = "BEGIN DEFERRED";
pub const ASSERT_CLEAN_ROLLBACK: &str = "ROLLBACK";

pub const UPSERT_QUALIFICATION_PROBE: &str = "INSERT INTO __attention_probe \
    (operation_id, fingerprint, value) VALUES (?1, ?2, ?3) \
    ON CONFLICT(operation_id) DO NOTHING";

pub const SELECT_QUALIFICATION_PROBE: &str = "SELECT fingerprint, value \
    FROM __attention_probe WHERE operation_id = ?1";

pub async fn configure_connection(
    connection: &Connection,
    busy_timeout: BusyTimeout,
) -> Result<(), Error> {
    connection
        .busy_timeout(busy_timeout.duration())
        .map_err(Error::from)?;
    connection.execute("PRAGMA foreign_keys = ON", ()).await?;
    Ok(())
}

#[cfg(test)]
pub async fn foreign_keys_enforced(connection: &Connection) -> Result<bool, Error> {
    connection
        .execute_batch(
            "DROP TABLE IF EXISTS temp.fk_child;\
             DROP TABLE IF EXISTS temp.fk_parent;\
             CREATE TEMP TABLE fk_parent (id INTEGER PRIMARY KEY);\
             CREATE TEMP TABLE fk_child (parent_id INTEGER REFERENCES fk_parent(id));",
        )
        .await?;
    let rejected = connection
        .execute("INSERT INTO fk_child (parent_id) VALUES (1)", ())
        .await
        .is_err();
    connection
        .execute_batch("DROP TABLE temp.fk_child; DROP TABLE temp.fk_parent;")
        .await?;
    Ok(rejected)
}
