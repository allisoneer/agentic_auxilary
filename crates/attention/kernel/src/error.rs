//! Native invariant failures.

use crate::ReminderFireId;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvariantError {
    #[error("UUID text is malformed: {0}")]
    InvalidUuidText(String),
    #[error("UUID text is not canonical lowercase hyphenated form: {0}")]
    NonCanonicalUuidText(String),
    #[error("UUID is not version 7")]
    InvalidUuidVersion,
    #[error("source component {component} cannot be empty")]
    EmptySourceComponent { component: &'static str },
    #[error("revision cannot be zero")]
    RevisionZero,
    #[error("revision overflow")]
    RevisionOverflow,
    #[error("invalid {entity} transition from {from} to {to}")]
    InvalidTransition {
        entity: &'static str,
        from: &'static str,
        to: &'static str,
    },
    #[error("reminder must retain at least one fire")]
    MissingReminderFire,
    #[error("duplicate reminder fire ID: {0}")]
    DuplicateReminderFireId(ReminderFireId),
    #[error("reminder has more than one scheduled or fired child")]
    MultipleCurrentReminderFires,
    #[error("unknown reminder fire ID: {0}")]
    UnknownReminderFire(ReminderFireId),
    #[error("duplicate reminder target")]
    DuplicateReminderTarget,
    #[error("snoozed reminder fire ID cannot be reused: {0}")]
    SnoozeIdReuse(ReminderFireId),
}
