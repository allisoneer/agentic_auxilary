//! Mutation idempotency and canonical command fingerprints.

use crate::InvariantError;
use sha2::Digest;
use sha2::Sha256;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;
use uuid::Version;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MutationIdempotencyKey(Uuid);

impl MutationIdempotencyKey {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for MutationIdempotencyKey {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<Uuid> for MutationIdempotencyKey {
    type Error = InvariantError;

    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        if value.get_version() != Some(Version::SortRand) {
            return Err(InvariantError::InvalidUuidVersion);
        }
        Ok(Self(value))
    }
}

impl FromStr for MutationIdempotencyKey {
    type Err = InvariantError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = Uuid::parse_str(value)
            .map_err(|_| InvariantError::InvalidUuidText(value.to_string()))?;
        if parsed.hyphenated().to_string() != value {
            return Err(InvariantError::NonCanonicalUuidText(value.to_string()));
        }
        Self::try_from(parsed)
    }
}

impl fmt::Display for MutationIdempotencyKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MutationOperation {
    CreateWorkItem,
    CompleteWorkItem,
    CancelWorkItem,
    AcknowledgeAttentionSignal,
    IngestSourceOccurrence,
    CreateReminder,
    FireReminder,
    AcknowledgeReminderFire,
    SnoozeReminderFire,
}

impl MutationOperation {
    pub const fn domain(self) -> &'static [u8] {
        match self {
            Self::CreateWorkItem => b"attention.create-work-item.v1",
            Self::CompleteWorkItem => b"attention.complete-work-item.v1",
            Self::CancelWorkItem => b"attention.cancel-work-item.v1",
            Self::AcknowledgeAttentionSignal => b"attention.acknowledge-signal.v1",
            Self::IngestSourceOccurrence => b"attention.ingest-source-occurrence.v1",
            Self::CreateReminder => b"attention.create-reminder.v1",
            Self::FireReminder => b"attention.fire-reminder.v1",
            Self::AcknowledgeReminderFire => b"attention.acknowledge-reminder-fire.v1",
            Self::SnoozeReminderFire => b"attention.snooze-reminder-fire.v1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CanonicalFingerprint([u8; 32]);

impl CanonicalFingerprint {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub const fn reconstruct(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

pub struct CanonicalWriter(Sha256);

impl CanonicalWriter {
    pub fn new(operation: MutationOperation) -> Self {
        let mut digest = Sha256::new();
        write_part(&mut digest, operation.domain());
        Self(digest)
    }

    pub fn bytes(&mut self, value: &[u8]) {
        write_part(&mut self.0, value);
    }

    pub fn bool(&mut self, value: bool) {
        self.bytes(&[u8::from(value)]);
    }

    pub fn optional_bytes(&mut self, value: Option<&[u8]>) {
        match value {
            Some(value) => {
                self.bool(true);
                self.bytes(value);
            }
            None => self.bool(false),
        }
    }

    pub fn finish(self) -> CanonicalFingerprint {
        CanonicalFingerprint::reconstruct(self.0.finalize().into())
    }
}

fn write_part(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

#[cfg(test)]
mod tests {
    use super::CanonicalWriter;
    use super::MutationOperation;

    #[test]
    fn canonical_writer_is_length_delimited_and_domain_separated() {
        let mut first = CanonicalWriter::new(MutationOperation::CreateWorkItem);
        first.bytes(b"ab");
        first.bytes(b"c");

        let mut differently_partitioned = CanonicalWriter::new(MutationOperation::CreateWorkItem);
        differently_partitioned.bytes(b"a");
        differently_partitioned.bytes(b"bc");

        let mut other_operation = CanonicalWriter::new(MutationOperation::CancelWorkItem);
        other_operation.bytes(b"ab");
        other_operation.bytes(b"c");
        other_operation.bool(true);
        other_operation.optional_bytes(None);

        assert_ne!(first.finish(), differently_partitioned.finish());
        assert_ne!(
            CanonicalWriter::new(MutationOperation::CreateWorkItem).finish(),
            other_operation.finish()
        );
    }
}
