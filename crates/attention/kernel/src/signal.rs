//! Attention signal root with independent source and human state.

use crate::AttentionSignalId;
use crate::InvariantError;
use crate::Revision;
use crate::SourceEntityId;
use crate::SourceReceiptId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalSourceLifecycle {
    Active,
    Resolved,
    Expired,
}

impl SignalSourceLifecycle {
    const fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Resolved => "resolved",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalAttentionState {
    Unread,
    Acknowledged,
}

impl SignalAttentionState {
    const fn name(self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::Acknowledged => "acknowledged",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionSignal {
    id: AttentionSignalId,
    revision: Revision,
    source_lifecycle: SignalSourceLifecycle,
    attention_state: SignalAttentionState,
    source_receipt_id: SourceReceiptId,
    source_entity_id: Option<SourceEntityId>,
}

impl AttentionSignal {
    pub const fn new(
        id: AttentionSignalId,
        source_receipt_id: SourceReceiptId,
        source_entity_id: Option<SourceEntityId>,
    ) -> Self {
        Self {
            id,
            revision: Revision::initial(),
            source_lifecycle: SignalSourceLifecycle::Active,
            attention_state: SignalAttentionState::Unread,
            source_receipt_id,
            source_entity_id,
        }
    }

    pub const fn reconstruct(
        id: AttentionSignalId,
        revision: Revision,
        source_lifecycle: SignalSourceLifecycle,
        attention_state: SignalAttentionState,
        source_receipt_id: SourceReceiptId,
        source_entity_id: Option<SourceEntityId>,
    ) -> Result<Self, InvariantError> {
        Ok(Self {
            id,
            revision,
            source_lifecycle,
            attention_state,
            source_receipt_id,
            source_entity_id,
        })
    }

    pub const fn id(&self) -> AttentionSignalId {
        self.id
    }

    pub const fn revision(&self) -> Revision {
        self.revision
    }

    pub const fn source_lifecycle(&self) -> SignalSourceLifecycle {
        self.source_lifecycle
    }

    pub const fn attention_state(&self) -> SignalAttentionState {
        self.attention_state
    }

    pub const fn source_receipt_id(&self) -> SourceReceiptId {
        self.source_receipt_id
    }

    pub const fn source_entity_id(&self) -> Option<SourceEntityId> {
        self.source_entity_id
    }

    pub fn resolve(&mut self) -> Result<(), InvariantError> {
        self.transition_source_to(SignalSourceLifecycle::Resolved)
    }

    pub fn expire(&mut self) -> Result<(), InvariantError> {
        self.transition_source_to(SignalSourceLifecycle::Expired)
    }

    pub fn acknowledge(&mut self) -> Result<(), InvariantError> {
        if self.attention_state != SignalAttentionState::Unread {
            return Err(InvariantError::InvalidTransition {
                entity: "signal attention",
                from: self.attention_state.name(),
                to: SignalAttentionState::Acknowledged.name(),
            });
        }
        self.revision = self.revision.checked_increment()?;
        self.attention_state = SignalAttentionState::Acknowledged;
        Ok(())
    }

    pub fn advance_source(
        &mut self,
        source_receipt_id: SourceReceiptId,
        source_entity_id: Option<SourceEntityId>,
        source_lifecycle: SignalSourceLifecycle,
        fresh_attention: bool,
    ) -> Result<(), InvariantError> {
        let revision = self.revision.checked_increment()?;
        self.source_receipt_id = source_receipt_id;
        self.source_entity_id = source_entity_id;
        self.source_lifecycle = source_lifecycle;
        if fresh_attention {
            self.attention_state = SignalAttentionState::Unread;
        }
        self.revision = revision;
        Ok(())
    }

    fn transition_source_to(&mut self, next: SignalSourceLifecycle) -> Result<(), InvariantError> {
        if self.source_lifecycle != SignalSourceLifecycle::Active {
            return Err(InvariantError::InvalidTransition {
                entity: "signal source",
                from: self.source_lifecycle.name(),
                to: next.name(),
            });
        }
        self.revision = self.revision.checked_increment()?;
        self.source_lifecycle = next;
        Ok(())
    }
}
