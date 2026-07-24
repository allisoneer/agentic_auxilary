//! Work item root and lifecycle.

use crate::InvariantError;
use crate::Revision;
use crate::SourceEntityKey;
use crate::WorkItemId;
use chrono::DateTime;
use chrono::Utc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkItemLifecycle {
    Open,
    Completed,
    Cancelled,
}

impl WorkItemLifecycle {
    const fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItem {
    id: WorkItemId,
    revision: Revision,
    lifecycle: WorkItemLifecycle,
    due_at: Option<DateTime<Utc>>,
    scheduled_at: Option<DateTime<Utc>>,
    defer_until: Option<DateTime<Utc>>,
    source_link: Option<SourceEntityKey>,
}

impl WorkItem {
    pub const fn new(
        id: WorkItemId,
        due_at: Option<DateTime<Utc>>,
        scheduled_at: Option<DateTime<Utc>>,
        defer_until: Option<DateTime<Utc>>,
        source_link: Option<SourceEntityKey>,
    ) -> Self {
        Self {
            id,
            revision: Revision::initial(),
            lifecycle: WorkItemLifecycle::Open,
            due_at,
            scheduled_at,
            defer_until,
            source_link,
        }
    }

    pub const fn reconstruct(
        id: WorkItemId,
        revision: Revision,
        lifecycle: WorkItemLifecycle,
        due_at: Option<DateTime<Utc>>,
        scheduled_at: Option<DateTime<Utc>>,
        defer_until: Option<DateTime<Utc>>,
        source_link: Option<SourceEntityKey>,
    ) -> Result<Self, InvariantError> {
        Ok(Self {
            id,
            revision,
            lifecycle,
            due_at,
            scheduled_at,
            defer_until,
            source_link,
        })
    }

    pub const fn id(&self) -> WorkItemId {
        self.id
    }

    pub const fn revision(&self) -> Revision {
        self.revision
    }

    pub const fn lifecycle(&self) -> WorkItemLifecycle {
        self.lifecycle
    }

    pub const fn due_at(&self) -> Option<&DateTime<Utc>> {
        self.due_at.as_ref()
    }

    pub const fn scheduled_at(&self) -> Option<&DateTime<Utc>> {
        self.scheduled_at.as_ref()
    }

    pub const fn defer_until(&self) -> Option<&DateTime<Utc>> {
        self.defer_until.as_ref()
    }

    pub const fn source_link(&self) -> Option<&SourceEntityKey> {
        self.source_link.as_ref()
    }

    pub fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        self.lifecycle == WorkItemLifecycle::Open && self.due_at.is_some_and(|due_at| due_at < now)
    }

    pub fn complete(&mut self) -> Result<(), InvariantError> {
        self.transition_to(WorkItemLifecycle::Completed)
    }

    pub fn cancel(&mut self) -> Result<(), InvariantError> {
        self.transition_to(WorkItemLifecycle::Cancelled)
    }

    fn transition_to(&mut self, next: WorkItemLifecycle) -> Result<(), InvariantError> {
        if self.lifecycle != WorkItemLifecycle::Open {
            return Err(InvariantError::InvalidTransition {
                entity: "work item",
                from: self.lifecycle.name(),
                to: next.name(),
            });
        }
        self.revision = self.revision.checked_increment()?;
        self.lifecycle = next;
        Ok(())
    }
}
