//! Native semantic change and synchronization contracts.

use crate::AttentionSignal;
use crate::ChangeEventId;
use crate::InboxEntry;
use crate::InvariantError;
use crate::Reminder;
use crate::SourceEntity;
use crate::SourceReceipt;
use crate::WorkItem;
use chrono::DateTime;
use chrono::Utc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommitCursor(u64);

impl CommitCursor {
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for CommitCursor {
    type Error = InvariantError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(InvariantError::CommitCursorZero);
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AffectedView {
    WorkItem { work_item: WorkItem },
    AttentionSignal { attention_signal: AttentionSignal },
    Reminder { reminder: Reminder },
    SourceReceipt { source_receipt: SourceReceipt },
    SourceEntity { source_entity: SourceEntity },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InboxEffects {
    additions: Vec<InboxEntry>,
    removals: Vec<InboxEntry>,
}

impl InboxEffects {
    pub fn new(additions: Vec<InboxEntry>, removals: Vec<InboxEntry>) -> Self {
        Self {
            additions,
            removals,
        }
    }

    pub fn additions(&self) -> &[InboxEntry] {
        &self.additions
    }

    pub fn removals(&self) -> &[InboxEntry] {
        &self.removals
    }

    pub fn is_empty(&self) -> bool {
        self.additions.is_empty() && self.removals.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeEventDraft {
    id: ChangeEventId,
    occurred_at: DateTime<Utc>,
    kind: ChangeKind,
    affected_views: Vec<AffectedView>,
    inbox_effects: InboxEffects,
}

impl ChangeEventDraft {
    pub fn new(
        id: ChangeEventId,
        occurred_at: DateTime<Utc>,
        kind: ChangeKind,
        affected_views: Vec<AffectedView>,
        inbox_effects: InboxEffects,
    ) -> Self {
        Self {
            id,
            occurred_at,
            kind,
            affected_views,
            inbox_effects,
        }
    }

    pub const fn id(&self) -> ChangeEventId {
        self.id
    }

    pub const fn occurred_at(&self) -> &DateTime<Utc> {
        &self.occurred_at
    }

    pub const fn kind(&self) -> ChangeKind {
        self.kind
    }

    pub fn affected_views(&self) -> &[AffectedView] {
        &self.affected_views
    }

    pub const fn inbox_effects(&self) -> &InboxEffects {
        &self.inbox_effects
    }

    pub fn commit(self, cursor: CommitCursor) -> ChangeEvent {
        ChangeEvent {
            cursor,
            draft: self,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeEvent {
    cursor: CommitCursor,
    draft: ChangeEventDraft,
}

impl ChangeEvent {
    pub const fn cursor(&self) -> CommitCursor {
        self.cursor
    }

    pub const fn draft(&self) -> &ChangeEventDraft {
        &self.draft
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionSnapshot {
    cursor: CommitCursor,
    work_items: Vec<WorkItem>,
    signals: Vec<AttentionSignal>,
    reminders: Vec<Reminder>,
}

impl AttentionSnapshot {
    pub fn new(
        cursor: CommitCursor,
        work_items: Vec<WorkItem>,
        signals: Vec<AttentionSignal>,
        reminders: Vec<Reminder>,
    ) -> Self {
        Self {
            cursor,
            work_items,
            signals,
            reminders,
        }
    }

    pub const fn cursor(&self) -> CommitCursor {
        self.cursor
    }

    pub fn work_items(&self) -> &[WorkItem] {
        &self.work_items
    }

    pub fn signals(&self) -> &[AttentionSignal] {
        &self.signals
    }

    pub fn reminders(&self) -> &[Reminder] {
        &self.reminders
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangePage {
    events: Vec<ChangeEvent>,
    resume_after: CommitCursor,
    has_more: bool,
}

impl ChangePage {
    pub fn new(events: Vec<ChangeEvent>, resume_after: CommitCursor, has_more: bool) -> Self {
        Self {
            events,
            resume_after,
            has_more,
        }
    }

    pub fn events(&self) -> &[ChangeEvent] {
        &self.events
    }

    pub const fn resume_after(&self) -> CommitCursor {
        self.resume_after
    }

    pub const fn has_more(&self) -> bool {
        self.has_more
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeGap {
    requested_after: CommitCursor,
    earliest_available: CommitCursor,
}

impl ChangeGap {
    pub const fn new(requested_after: CommitCursor, earliest_available: CommitCursor) -> Self {
        Self {
            requested_after,
            earliest_available,
        }
    }

    pub const fn requested_after(self) -> CommitCursor {
        self.requested_after
    }

    pub const fn earliest_available(self) -> CommitCursor {
        self.earliest_available
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangesResult {
    Page(ChangePage),
    Gap(ChangeGap),
}
