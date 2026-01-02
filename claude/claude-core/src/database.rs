//! Database wrapper for libsql/Turso

use crate::CoreError;
use libsql::{Builder, Connection, Database as LibsqlDatabase};

/// Database wrapper with schema management
pub struct Database {
    db: LibsqlDatabase,
}

impl Database {
    /// Open or create a database at the given path
    pub async fn open(path: &str) -> Result<Self, CoreError> {
        let db = Builder::new_local(path)
            .build()
            .await
            .map_err(|e| CoreError::Db(e.to_string()))?;

        let conn = db.connect().map_err(|e| CoreError::Db(e.to_string()))?;

        // Create schema
        conn.execute(
            r"
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                model TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
            ",
            (),
        )
        .await
        .map_err(|e| CoreError::Db(e.to_string()))?;

        conn.execute(
            r"
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY(conversation_id) REFERENCES conversations(id)
            )
            ",
            (),
        )
        .await
        .map_err(|e| CoreError::Db(e.to_string()))?;

        conn.execute(
            r"
            CREATE INDEX IF NOT EXISTS idx_messages_conversation 
            ON messages(conversation_id, created_at)
            ",
            (),
        )
        .await
        .map_err(|e| CoreError::Db(e.to_string()))?;

        Ok(Self { db })
    }

    /// Get a connection to the database
    pub fn connect(&self) -> Result<Connection, CoreError> {
        self.db.connect().map_err(|e| CoreError::Db(e.to_string()))
    }
}
