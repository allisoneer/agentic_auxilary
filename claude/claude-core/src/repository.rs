//! Repository for conversation and message CRUD

use crate::{
    database::Database,
    state::{Conversation, Message},
    CoreError, DEFAULT_MODEL,
};
use libsql::Connection;
use uuid::Uuid;

/// Repository for database operations
pub struct Repository {
    conn: Connection,
}

impl Repository {
    /// Create a new repository with the given database
    pub fn new(db: &Database) -> Result<Self, CoreError> {
        Ok(Self { conn: db.connect()? })
    }

    /// Create a new conversation
    pub async fn create_conversation(&self, title: &str) -> Result<Conversation, CoreError> {
        let now = chrono::Utc::now().timestamp();
        let id = Uuid::new_v4().to_string();

        self.conn
            .execute(
                "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                (id.as_str(), title, DEFAULT_MODEL, now, now),
            )
            .await
            .map_err(|e| CoreError::Db(e.to_string()))?;

        Ok(Conversation {
            id,
            title: title.to_string(),
            model: DEFAULT_MODEL.to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    /// List all conversations ordered by most recent first
    pub async fn list_conversations(&self) -> Result<Vec<Conversation>, CoreError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, title, model, created_at, updated_at FROM conversations ORDER BY updated_at DESC",
                (),
            )
            .await
            .map_err(|e| CoreError::Db(e.to_string()))?;

        let mut conversations = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| CoreError::Db(e.to_string()))? {
            conversations.push(Conversation {
                id: row.get::<String>(0).map_err(|e| CoreError::Db(e.to_string()))?,
                title: row.get::<String>(1).map_err(|e| CoreError::Db(e.to_string()))?,
                model: row.get::<String>(2).map_err(|e| CoreError::Db(e.to_string()))?,
                created_at: row.get::<i64>(3).map_err(|e| CoreError::Db(e.to_string()))?,
                updated_at: row.get::<i64>(4).map_err(|e| CoreError::Db(e.to_string()))?,
            });
        }

        Ok(conversations)
    }

    /// Delete a conversation and its messages
    pub async fn delete_conversation(&self, id: &str) -> Result<(), CoreError> {
        self.conn
            .execute("DELETE FROM messages WHERE conversation_id = ?1", [id])
            .await
            .map_err(|e| CoreError::Db(e.to_string()))?;

        self.conn
            .execute("DELETE FROM conversations WHERE id = ?1", [id])
            .await
            .map_err(|e| CoreError::Db(e.to_string()))?;

        Ok(())
    }

    /// Update conversation title
    pub async fn update_conversation_title(&self, id: &str, title: &str) -> Result<(), CoreError> {
        let now = chrono::Utc::now().timestamp();
        self.conn
            .execute(
                "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
                (title, now, id),
            )
            .await
            .map_err(|e| CoreError::Db(e.to_string()))?;

        Ok(())
    }

    /// Append a message to a conversation
    pub async fn append_message(
        &self,
        conversation_id: &str,
        msg: &Message,
    ) -> Result<(), CoreError> {
        self.conn
            .execute(
                "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                (msg.id.as_str(), conversation_id, msg.role.as_str(), msg.content.as_str(), msg.created_at),
            )
            .await
            .map_err(|e| CoreError::Db(e.to_string()))?;

        let now = chrono::Utc::now().timestamp();
        self.conn
            .execute(
                "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
                (now, conversation_id),
            )
            .await
            .map_err(|e| CoreError::Db(e.to_string()))?;

        Ok(())
    }

    /// List messages for a conversation
    pub async fn list_messages(&self, conversation_id: &str) -> Result<Vec<Message>, CoreError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, role, content, created_at FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
                [conversation_id],
            )
            .await
            .map_err(|e| CoreError::Db(e.to_string()))?;

        let mut messages = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| CoreError::Db(e.to_string()))? {
            messages.push(Message {
                id: row.get::<String>(0).map_err(|e| CoreError::Db(e.to_string()))?,
                role: row.get::<String>(1).map_err(|e| CoreError::Db(e.to_string()))?,
                content: row.get::<String>(2).map_err(|e| CoreError::Db(e.to_string()))?,
                created_at: row.get::<i64>(3).map_err(|e| CoreError::Db(e.to_string()))?,
            });
        }

        Ok(messages)
    }
}
