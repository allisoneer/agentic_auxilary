//! Reminder root and retained durable fire children.

use crate::AttentionSignalId;
use crate::InvariantError;
use crate::ReminderFireId;
use crate::ReminderId;
use crate::Revision;
use crate::WorkItemId;
use chrono::DateTime;
use chrono::Utc;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReminderTarget {
    WorkItem(WorkItemId),
    AttentionSignal(AttentionSignalId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReminderFireState {
    Scheduled,
    Fired,
    Acknowledged,
    Snoozed,
}

impl ReminderFireState {
    const fn name(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Fired => "fired",
            Self::Acknowledged => "acknowledged",
            Self::Snoozed => "snoozed",
        }
    }

    const fn is_current(self) -> bool {
        matches!(self, Self::Scheduled | Self::Fired)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderFire {
    id: ReminderFireId,
    trigger_at: DateTime<Utc>,
    state: ReminderFireState,
}

impl ReminderFire {
    pub const fn reconstruct(
        id: ReminderFireId,
        trigger_at: DateTime<Utc>,
        state: ReminderFireState,
    ) -> Result<Self, InvariantError> {
        Ok(Self {
            id,
            trigger_at,
            state,
        })
    }

    const fn scheduled(id: ReminderFireId, trigger_at: DateTime<Utc>) -> Self {
        Self {
            id,
            trigger_at,
            state: ReminderFireState::Scheduled,
        }
    }

    pub const fn id(&self) -> ReminderFireId {
        self.id
    }

    pub const fn trigger_at(&self) -> &DateTime<Utc> {
        &self.trigger_at
    }

    pub const fn state(&self) -> ReminderFireState {
        self.state
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    id: ReminderId,
    revision: Revision,
    target: ReminderTarget,
    trigger_at: DateTime<Utc>,
    fires: Vec<ReminderFire>,
}

impl Reminder {
    pub fn new(
        id: ReminderId,
        target: ReminderTarget,
        trigger_at: DateTime<Utc>,
        fire_id: ReminderFireId,
    ) -> Self {
        Self {
            id,
            revision: Revision::initial(),
            target,
            trigger_at,
            fires: vec![ReminderFire::scheduled(fire_id, trigger_at)],
        }
    }

    pub fn reconstruct(
        id: ReminderId,
        revision: Revision,
        target: ReminderTarget,
        trigger_at: DateTime<Utc>,
        fires: Vec<ReminderFire>,
    ) -> Result<Self, InvariantError> {
        validate_fires(&fires)?;
        Ok(Self {
            id,
            revision,
            target,
            trigger_at,
            fires,
        })
    }

    pub const fn id(&self) -> ReminderId {
        self.id
    }

    pub const fn revision(&self) -> Revision {
        self.revision
    }

    pub const fn target(&self) -> ReminderTarget {
        self.target
    }

    pub const fn trigger_at(&self) -> &DateTime<Utc> {
        &self.trigger_at
    }

    pub fn fires(&self) -> &[ReminderFire] {
        &self.fires
    }

    pub fn mark_fired(&mut self, fire_id: ReminderFireId) -> Result<(), InvariantError> {
        let index = self.fire_index(fire_id)?;
        self.require_state(
            index,
            ReminderFireState::Scheduled,
            ReminderFireState::Fired,
        )?;
        let revision = self.revision.checked_increment()?;
        self.fires[index].state = ReminderFireState::Fired;
        self.revision = revision;
        Ok(())
    }

    pub fn acknowledge_fire(&mut self, fire_id: ReminderFireId) -> Result<(), InvariantError> {
        let index = self.fire_index(fire_id)?;
        self.require_state(
            index,
            ReminderFireState::Fired,
            ReminderFireState::Acknowledged,
        )?;
        let revision = self.revision.checked_increment()?;
        self.fires[index].state = ReminderFireState::Acknowledged;
        self.revision = revision;
        Ok(())
    }

    pub fn snooze_fire(
        &mut self,
        fire_id: ReminderFireId,
        new_fire_id: ReminderFireId,
        trigger_at: DateTime<Utc>,
    ) -> Result<(), InvariantError> {
        if self.fires.iter().any(|fire| fire.id == new_fire_id) {
            return Err(InvariantError::SnoozeIdReuse(new_fire_id));
        }
        let index = self.fire_index(fire_id)?;
        self.require_state(index, ReminderFireState::Fired, ReminderFireState::Snoozed)?;
        let revision = self.revision.checked_increment()?;
        self.fires[index].state = ReminderFireState::Snoozed;
        self.fires
            .push(ReminderFire::scheduled(new_fire_id, trigger_at));
        self.revision = revision;
        Ok(())
    }

    fn fire_index(&self, fire_id: ReminderFireId) -> Result<usize, InvariantError> {
        self.fires
            .iter()
            .position(|fire| fire.id == fire_id)
            .ok_or(InvariantError::UnknownReminderFire(fire_id))
    }

    fn require_state(
        &self,
        index: usize,
        required: ReminderFireState,
        next: ReminderFireState,
    ) -> Result<(), InvariantError> {
        let current = self.fires[index].state;
        if current != required {
            return Err(InvariantError::InvalidTransition {
                entity: "reminder fire",
                from: current.name(),
                to: next.name(),
            });
        }
        Ok(())
    }
}

pub fn validate_unique_reminder_targets(reminders: &[Reminder]) -> Result<(), InvariantError> {
    let mut targets = HashSet::with_capacity(reminders.len());
    for reminder in reminders {
        if !targets.insert(reminder.target) {
            return Err(InvariantError::DuplicateReminderTarget);
        }
    }
    Ok(())
}

fn validate_fires(fires: &[ReminderFire]) -> Result<(), InvariantError> {
    if fires.is_empty() {
        return Err(InvariantError::MissingReminderFire);
    }

    let mut ids = HashSet::with_capacity(fires.len());
    let mut current_count = 0;
    for fire in fires {
        if !ids.insert(fire.id) {
            return Err(InvariantError::DuplicateReminderFireId(fire.id));
        }
        if fire.state.is_current() {
            current_count += 1;
        }
    }

    if current_count > 1 {
        return Err(InvariantError::MultipleCurrentReminderFires);
    }
    Ok(())
}
