//! Snapshot and retained-change synchronization contracts.

use crate::AttentionSignalView;
use crate::ChangeEvent;
use crate::Cursor;
use crate::InboxEntryView;
use crate::ReminderView;
use crate::WorkItemView;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de;

/// Cursor-free complete Attention state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionSnapshot {
    pub work_items: Vec<WorkItemView>,
    pub attention_signals: Vec<AttentionSignalView>,
    pub reminders: Vec<ReminderView>,
    pub inbox: Vec<InboxEntryView>,
}

/// Point-query snapshot result with an unambiguous post-state cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotResult {
    pub state: AttentionSnapshot,
    pub after_cursor: Cursor,
}

/// Reason a requested replay cursor cannot be served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorGapReason {
    Invalid,
    Future,
    Expired,
}

/// Wire invariant that serializes and deserializes only as `true`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SnapshotRequired;

impl Serialize for SnapshotRequired {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(true)
    }
}

impl<'de> Deserialize<'de> for SnapshotRequired {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if bool::deserialize(deserializer)? {
            Ok(Self)
        } else {
            Err(de::Error::custom("snapshot_required must be true"))
        }
    }
}

/// Explicit reset instruction for invalid, future, or expired cursors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorGap {
    pub reason: CursorGapReason,
    pub requested_after: Cursor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earliest_available: Option<Cursor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_available: Option<Cursor>,
    pub snapshot_required: SnapshotRequired,
}

/// Result of requesting retained changes after a cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangesResult {
    Page {
        events: Vec<ChangeEvent>,
        resume_after: Cursor,
        has_more: bool,
    },
    Gap {
        #[serde(flatten)]
        gap: CursorGap,
    },
}
