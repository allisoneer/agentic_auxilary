//! Deterministic semantic rejections and adapter-neutral port failures.

use crate::AttentionSignalId;
use crate::MutationIdempotencyKey;
use crate::OccurrenceKey;
use crate::ReminderFireId;
use crate::ReminderId;
use crate::Revision;
use crate::SourceEntityKey;
use crate::SourceStateVersion;
use crate::WorkItemId;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceRef {
    WorkItem(WorkItemId),
    AttentionSignal(AttentionSignalId),
    Reminder(ReminderId),
    ReminderFire(ReminderFireId),
    SourceEntity(SourceEntityKey),
    SourceOccurrence(OccurrenceKey),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticError {
    NotFound(ResourceRef),
    ExpectedRevisionConflict {
        resource: ResourceRef,
        expected: Revision,
        actual: Revision,
    },
    CreateConflict(ResourceRef),
    IdempotencyMismatch(MutationIdempotencyKey),
    OccurrenceContentMismatch(OccurrenceKey),
    ObservedSourceVersionConflict {
        entity: SourceEntityKey,
        observed: Option<SourceStateVersion>,
        actual: Option<SourceStateVersion>,
    },
}

impl fmt::Display for SemanticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "semantic mutation rejected: {self:?}")
    }
}

impl Error for SemanticError {}

#[derive(Debug)]
pub enum PortError<E> {
    Semantic(SemanticError),
    Adapter(E),
}

impl<E: fmt::Display> fmt::Display for PortError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Semantic(error) => error.fmt(formatter),
            Self::Adapter(error) => write!(formatter, "adapter operation failed: {error}"),
        }
    }
}

impl<E: Error + 'static> Error for PortError<E> {}
