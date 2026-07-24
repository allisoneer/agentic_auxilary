//! Pure default Inbox projections.

use crate::AttentionSignal;
use crate::AttentionSignalId;
use crate::ReminderFire;
use crate::ReminderFireId;
use crate::ReminderFireState;
use crate::SignalAttentionState;
use crate::WorkItem;
use crate::WorkItemId;
use crate::WorkItemLifecycle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InboxEntry {
    WorkItem(WorkItemId),
    AttentionSignal(AttentionSignalId),
    ReminderFire(ReminderFireId),
}

impl WorkItem {
    pub const fn is_in_default_inbox(&self) -> bool {
        matches!(self.lifecycle(), WorkItemLifecycle::Open)
    }

    pub const fn inbox_entry(&self) -> Option<InboxEntry> {
        if self.is_in_default_inbox() {
            Some(InboxEntry::WorkItem(self.id()))
        } else {
            None
        }
    }
}

impl AttentionSignal {
    pub const fn is_in_default_inbox(&self) -> bool {
        matches!(self.attention_state(), SignalAttentionState::Unread)
    }

    pub const fn inbox_entry(&self) -> Option<InboxEntry> {
        if self.is_in_default_inbox() {
            Some(InboxEntry::AttentionSignal(self.id()))
        } else {
            None
        }
    }
}

impl ReminderFire {
    pub const fn is_in_default_inbox(&self) -> bool {
        matches!(self.state(), ReminderFireState::Fired)
    }

    pub const fn inbox_entry(&self) -> Option<InboxEntry> {
        if self.is_in_default_inbox() {
            Some(InboxEntry::ReminderFire(self.id()))
        } else {
            None
        }
    }
}
