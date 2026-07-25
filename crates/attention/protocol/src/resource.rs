//! Independent public Attention resource and Inbox views.

use crate::AttentionSignalId;
use crate::NormalizedSourceOrder;
use crate::ReminderFireId;
use crate::ReminderId;
use crate::Revision;
use crate::SourceEntityId;
use crate::SourceReceiptId;
use crate::SourceStateVersion;
use crate::WireTimestamp;
use crate::WorkItemId;
use serde::Deserialize;
use serde::Serialize;

/// Frozen resource and Inbox variant names governed by v1 fixtures.
pub const V1_RESOURCE_VARIANT_NAMES: &[&str] = &[
    "work_item:open",
    "work_item:completed",
    "work_item:cancelled",
    "signal_source:active",
    "signal_source:resolved",
    "signal_source:expired",
    "signal_attention:unread",
    "signal_attention:acknowledged",
    "reminder_fire:scheduled",
    "reminder_fire:fired",
    "reminder_fire:acknowledged",
    "reminder_fire:snoozed",
    "source_order:unordered",
    "source_order:ordered",
    "inbox:work_item",
    "inbox:attention_signal",
    "inbox:reminder_fire",
];

macro_rules! source_component {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// Returns the opaque component value.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

source_component!(
    /// A source adapter kind.
    SourceKind
);
source_component!(
    /// A source adapter instance.
    SourceInstance
);
source_component!(
    /// A source-defined occurrence identity.
    OccurrenceId
);
source_component!(
    /// A source-defined entity identity.
    ExternalEntityId
);
source_component!(
    /// The comparison domain for ordered source values.
    SourceOrderDomain
);

/// Globally names one immutable source occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OccurrenceKey {
    pub source_kind: SourceKind,
    pub source_instance: SourceInstance,
    pub occurrence_id: OccurrenceId,
}

/// Globally names one mutable source entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceEntityKey {
    pub source_kind: SourceKind,
    pub source_instance: SourceInstance,
    pub external_entity_id: ExternalEntityId,
}

/// Source ordering metadata attached to occurrences and entities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SourceOrder {
    Unordered,
    Ordered {
        domain: SourceOrderDomain,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<NormalizedSourceOrder>,
    },
}

/// `WorkItem` lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemLifecycle {
    Open,
    Completed,
    Cancelled,
}

/// Complete bounded `WorkItem` view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemView {
    pub id: WorkItemId,
    pub revision: Revision,
    pub lifecycle: WorkItemLifecycle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_at: Option<WireTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<WireTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_until: Option<WireTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_link: Option<SourceEntityKey>,
}

/// Independent source lifecycle for an `AttentionSignal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalSourceLifecycle {
    Active,
    Resolved,
    Expired,
}

/// Human attention state for an `AttentionSignal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalAttentionState {
    Unread,
    Acknowledged,
}

/// Complete bounded `AttentionSignal` view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionSignalView {
    pub id: AttentionSignalId,
    pub revision: Revision,
    pub source_lifecycle: SignalSourceLifecycle,
    pub attention_state: SignalAttentionState,
    pub source_receipt_id: SourceReceiptId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_entity_id: Option<SourceEntityId>,
}

/// Resource targeted by a Reminder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReminderTarget {
    WorkItem {
        work_item_id: WorkItemId,
    },
    AttentionSignal {
        attention_signal_id: AttentionSignalId,
    },
}

/// Durable `ReminderFire` lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderFireState {
    Scheduled,
    Fired,
    Acknowledged,
    Snoozed,
}

/// Complete retained `ReminderFire` child view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderFireView {
    pub id: ReminderFireId,
    pub trigger_at: WireTimestamp,
    pub state: ReminderFireState,
}

/// Complete Reminder root including retained fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderView {
    pub id: ReminderId,
    pub revision: Revision,
    pub target: ReminderTarget,
    pub trigger_at: WireTimestamp,
    pub fires: Vec<ReminderFireView>,
}

/// Bounded immutable source-receipt view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceReceiptView {
    pub id: SourceReceiptId,
    pub occurrence_key: OccurrenceKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_entity_key: Option<SourceEntityKey>,
    pub source_order: SourceOrder,
    pub occurred_at: WireTimestamp,
    pub ingested_at: WireTimestamp,
}

/// Current source-entity authority view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEntityView {
    pub id: SourceEntityId,
    pub key: SourceEntityKey,
    pub version: SourceStateVersion,
    pub latest_receipt_id: SourceReceiptId,
    pub order: SourceOrder,
}

/// Complete materialized default-Inbox entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboxEntryView {
    WorkItem {
        work_item: WorkItemView,
    },
    AttentionSignal {
        attention_signal: AttentionSignalView,
    },
    ReminderFire {
        reminder_id: ReminderId,
        reminder_revision: Revision,
        target: ReminderTarget,
        fire: ReminderFireView,
    },
}

/// Typed key for removing a materialized Inbox entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboxEntryKey {
    WorkItem {
        work_item_id: WorkItemId,
    },
    AttentionSignal {
        attention_signal_id: AttentionSignalId,
    },
    ReminderFire {
        reminder_fire_id: ReminderFireId,
    },
}
