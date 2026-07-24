//! Native semantic types and pure invariants for Attention.
//!
//! This crate is independent from transport, persistence, scheduling, providers, and UI.

mod error;
mod id;
mod inbox;
mod reminder;
mod revision;
mod signal;
mod source;
mod work_item;

pub use error::InvariantError;
pub use id::AttentionSignalId;
pub use id::ChangeEventId;
pub use id::OutboxIntentId;
pub use id::ReminderFireId;
pub use id::ReminderId;
pub use id::SourceEntityId;
pub use id::SourceReceiptId;
pub use id::WorkItemId;
pub use inbox::InboxEntry;
pub use reminder::Reminder;
pub use reminder::ReminderFire;
pub use reminder::ReminderFireState;
pub use reminder::ReminderTarget;
pub use reminder::validate_unique_reminder_targets;
pub use revision::Revision;
pub use signal::AttentionSignal;
pub use signal::SignalAttentionState;
pub use signal::SignalSourceLifecycle;
pub use source::ExternalEntityId;
pub use source::OccurrenceId;
pub use source::OccurrenceKey;
pub use source::SourceEntity;
pub use source::SourceEntityKey;
pub use source::SourceInstance;
pub use source::SourceKind;
pub use source::SourceReceipt;
pub use work_item::WorkItem;
pub use work_item::WorkItemLifecycle;
