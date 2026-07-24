//! Immutable reducer-ready synchronization events.

use crate::AttentionSignalView;
use crate::ChangeEventId;
use crate::Cursor;
use crate::InboxEntryKey;
use crate::InboxEntryView;
use crate::ReminderView;
use crate::SourceEntityView;
use crate::SourceReceiptView;
use crate::WireTimestamp;
use crate::WorkItemView;
use serde::Deserialize;
use serde::Serialize;

/// Frozen v1 semantic event taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    WorkItemCreated,
    WorkItemCompleted,
    WorkItemCancelled,
    AttentionSignalAcknowledged,
    SourceOccurrenceIngested,
    ReminderCreated,
    ReminderFired,
    ReminderFireAcknowledged,
    ReminderFireSnoozed,
}

/// Exact v1 event-kind wire names, used by conformance governance.
pub const V1_CHANGE_KIND_NAMES: &[&str] = &[
    "work_item_created",
    "work_item_completed",
    "work_item_cancelled",
    "attention_signal_acknowledged",
    "source_occurrence_ingested",
    "reminder_created",
    "reminder_fired",
    "reminder_fire_acknowledged",
    "reminder_fire_snoozed",
];

/// Complete post-commit view affected by a semantic mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AffectedView {
    WorkItem {
        work_item: WorkItemView,
    },
    AttentionSignal {
        attention_signal: AttentionSignalView,
    },
    Reminder {
        reminder: ReminderView,
    },
    SourceReceipt {
        source_receipt: SourceReceiptView,
    },
    SourceEntity {
        source_entity: SourceEntityView,
    },
}

/// Authoritative materialized-Inbox effects from one mutation.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InboxEffects {
    pub upserts: Vec<InboxEntryView>,
    pub removals: Vec<InboxEntryKey>,
}

/// One immutable retained protocol change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEvent {
    pub id: ChangeEventId,
    pub cursor: Cursor,
    pub occurred_at: WireTimestamp,
    pub kind: ChangeKind,
    pub affected: Vec<AffectedView>,
    pub inbox: InboxEffects,
}
