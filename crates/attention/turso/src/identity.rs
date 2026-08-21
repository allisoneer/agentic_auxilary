use crate::Error;
use crate::writer::TransactionDecision;
use crate::writer::Writer;
use futures::FutureExt;
use tokio::sync::Mutex;
use turso_db::Database;
use turso_db::params;
use uuid::Uuid;
use uuid::Version;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistentServerIdentity {
    pub server_id: Uuid,
    pub stream_id: Uuid,
}

impl PersistentServerIdentity {
    pub fn new(server_id: Uuid, stream_id: Uuid) -> Result<Self, Error> {
        validate(server_id)?;
        validate(stream_id)?;
        if server_id == stream_id {
            return Err(Error::InvariantViolation(
                "server and stream identities must be distinct",
            ));
        }
        Ok(Self {
            server_id,
            stream_id,
        })
    }

    #[must_use]
    pub fn generate() -> Self {
        Self {
            server_id: Uuid::now_v7(),
            stream_id: Uuid::now_v7(),
        }
    }
}

fn validate(id: Uuid) -> Result<(), Error> {
    if id.get_version() != Some(Version::SortRand) {
        return Err(Error::InvariantViolation(
            "persistent server identities must be UUIDv7",
        ));
    }
    Ok(())
}

pub async fn load_or_create(
    writer: &Writer,
    engine: &Mutex<Option<Database>>,
    candidate: PersistentServerIdentity,
) -> Result<PersistentServerIdentity, Error> {
    validate(candidate.server_id)?;
    validate(candidate.stream_id)?;
    if candidate.server_id == candidate.stream_id {
        return Err(Error::InvariantViolation(
            "server and stream identities must be distinct",
        ));
    }
    writer
        .with_immediate(engine, |transaction| {
            async move {
                transaction
                    .execute(
                        "INSERT INTO attention_server_identity (singleton, server_id, stream_id) \
                         VALUES (1, ?1, ?2) ON CONFLICT(singleton) DO NOTHING",
                        params![
                            candidate.server_id.to_string(),
                            candidate.stream_id.to_string()
                        ],
                    )
                    .await
                    .map_err(Error::from)
                    .map_err(attention_kernel::PortError::Adapter)?;
                let mut rows = transaction
                    .query(
                        "SELECT server_id, stream_id FROM attention_server_identity \
                         WHERE singleton = 1",
                        (),
                    )
                    .await
                    .map_err(Error::from)
                    .map_err(attention_kernel::PortError::Adapter)?;
                let row = rows
                    .next()
                    .await
                    .map_err(Error::from)
                    .map_err(attention_kernel::PortError::Adapter)?
                    .ok_or(attention_kernel::PortError::Adapter(
                        Error::InvariantViolation("persistent server identity row is missing"),
                    ))?;
                let server_id = row
                    .get::<String>(0)
                    .map_err(Error::from)
                    .map_err(attention_kernel::PortError::Adapter)?;
                let stream_id = row
                    .get::<String>(1)
                    .map_err(Error::from)
                    .map_err(attention_kernel::PortError::Adapter)?;
                drop(rows);
                let stored = PersistentServerIdentity::new(
                    Uuid::parse_str(&server_id).map_err(|error| {
                        attention_kernel::PortError::Adapter(Error::Decode(Box::new(error)))
                    })?,
                    Uuid::parse_str(&stream_id).map_err(|error| {
                        attention_kernel::PortError::Adapter(Error::Decode(Box::new(error)))
                    })?,
                )
                .map_err(attention_kernel::PortError::Adapter)?;
                Ok(TransactionDecision::Commit(stored))
            }
            .boxed()
        })
        .await
        .map_err(|error| match error {
            attention_kernel::PortError::Adapter(error) => error,
            attention_kernel::PortError::Semantic(_) => {
                Error::InvariantViolation("identity transaction returned semantic error")
            }
        })
}
