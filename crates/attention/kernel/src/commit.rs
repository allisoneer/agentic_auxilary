//! Complete operation-specific atomic commit bundles.

use crate::AttentionSignal;
use crate::CanonicalFingerprint;
use crate::ChangeEventDraft;
use crate::MutationIdempotencyKey;
use crate::MutationOperation;
use crate::ObservedSourceAuthority;
use crate::OccurrenceKey;
use crate::OutboxIntent;
use crate::Reminder;
use crate::ReminderFireId;
use crate::ReminderTarget;
use crate::ResourceRef;
use crate::Revision;
use crate::SourceEntity;
use crate::SourceEntityKey;
use crate::SourceIngestionValue;
use crate::SourceReceipt;
use crate::WorkItem;
use crate::result::AttentionSignalValue;
use crate::result::ReminderValue;
use crate::result::WorkItemValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyCommit {
    key: MutationIdempotencyKey,
    operation: MutationOperation,
    fingerprint: CanonicalFingerprint,
}

impl IdempotencyCommit {
    pub const fn new(
        key: MutationIdempotencyKey,
        operation: MutationOperation,
        fingerprint: CanonicalFingerprint,
    ) -> Self {
        Self {
            key,
            operation,
            fingerprint,
        }
    }

    pub const fn key(&self) -> MutationIdempotencyKey {
        self.key
    }

    pub const fn operation(&self) -> MutationOperation {
        self.operation
    }

    pub const fn fingerprint(&self) -> CanonicalFingerprint {
        self.fingerprint
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedRevisionGuard {
    resource: ResourceRef,
    expected: Revision,
}

impl ExpectedRevisionGuard {
    pub const fn new(resource: ResourceRef, expected: Revision) -> Self {
        Self { resource, expected }
    }

    pub const fn resource(&self) -> &ResourceRef {
        &self.resource
    }

    pub const fn expected(&self) -> Revision {
        self.expected
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateGuard {
    resource: ResourceRef,
}

impl CreateGuard {
    pub const fn absent(resource: ResourceRef) -> Self {
        Self { resource }
    }

    pub const fn resource(&self) -> &ResourceRef {
        &self.resource
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccurrenceGuard {
    key: OccurrenceKey,
    fingerprint: CanonicalFingerprint,
}

impl OccurrenceGuard {
    pub const fn new(key: OccurrenceKey, fingerprint: CanonicalFingerprint) -> Self {
        Self { key, fingerprint }
    }

    pub const fn key(&self) -> &OccurrenceKey {
        &self.key
    }

    pub const fn fingerprint(&self) -> CanonicalFingerprint {
        self.fingerprint
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceAuthorityGuard {
    key: Option<SourceEntityKey>,
    observed: ObservedSourceAuthority,
}

impl SourceAuthorityGuard {
    pub const fn new(key: Option<SourceEntityKey>, observed: ObservedSourceAuthority) -> Self {
        Self { key, observed }
    }

    pub const fn key(&self) -> Option<&SourceEntityKey> {
        self.key.as_ref()
    }

    pub const fn observed(&self) -> &ObservedSourceAuthority {
        &self.observed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReminderTargetGuard(ReminderTarget);

impl ReminderTargetGuard {
    pub const fn unique(target: ReminderTarget) -> Self {
        Self(target)
    }

    pub const fn target(self) -> ReminderTarget {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentFireGuard {
    fire_id: ReminderFireId,
}

impl CurrentFireGuard {
    pub const fn new(fire_id: ReminderFireId) -> Self {
        Self { fire_id }
    }

    pub const fn fire_id(self) -> ReminderFireId {
        self.fire_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicEffects {
    change: ChangeEventDraft,
    outbox_intent: Option<OutboxIntent>,
}

impl AtomicEffects {
    pub const fn new(change: ChangeEventDraft, outbox_intent: Option<OutboxIntent>) -> Self {
        Self {
            change,
            outbox_intent,
        }
    }

    pub const fn change(&self) -> &ChangeEventDraft {
        &self.change
    }

    pub const fn outbox_intent(&self) -> Option<&OutboxIntent> {
        self.outbox_intent.as_ref()
    }
}

macro_rules! root_bundle {
    ($name:ident, $root:ty, $value:ty, $guard:ty) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            guard: $guard,
            idempotency: IdempotencyCommit,
            root: $root,
            value: $value,
            effects: AtomicEffects,
        }

        impl $name {
            pub const fn new(
                guard: $guard,
                idempotency: IdempotencyCommit,
                root: $root,
                value: $value,
                effects: AtomicEffects,
            ) -> Self {
                Self {
                    guard,
                    idempotency,
                    root,
                    value,
                    effects,
                }
            }

            pub const fn guard(&self) -> &$guard {
                &self.guard
            }

            pub const fn idempotency(&self) -> &IdempotencyCommit {
                &self.idempotency
            }

            pub const fn root(&self) -> &$root {
                &self.root
            }

            pub const fn value(&self) -> &$value {
                &self.value
            }

            pub const fn effects(&self) -> &AtomicEffects {
                &self.effects
            }
        }
    };
}

root_bundle!(CreateWorkItemBundle, WorkItem, WorkItemValue, CreateGuard);
root_bundle!(
    CompleteWorkItemBundle,
    WorkItem,
    WorkItemValue,
    ExpectedRevisionGuard
);
root_bundle!(
    CancelWorkItemBundle,
    WorkItem,
    WorkItemValue,
    ExpectedRevisionGuard
);
root_bundle!(
    AcknowledgeAttentionSignalBundle,
    AttentionSignal,
    AttentionSignalValue,
    ExpectedRevisionGuard
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestSourceOccurrenceBundle {
    occurrence_guard: OccurrenceGuard,
    authority_guard: SourceAuthorityGuard,
    idempotency: IdempotencyCommit,
    receipt: SourceReceipt,
    entity: Option<SourceEntity>,
    signal: Option<AttentionSignal>,
    value: SourceIngestionValue,
    effects: AtomicEffects,
}

impl IngestSourceOccurrenceBundle {
    #[expect(clippy::too_many_arguments, reason = "complete atomic source commit")]
    pub const fn new(
        occurrence_guard: OccurrenceGuard,
        authority_guard: SourceAuthorityGuard,
        idempotency: IdempotencyCommit,
        receipt: SourceReceipt,
        entity: Option<SourceEntity>,
        signal: Option<AttentionSignal>,
        value: SourceIngestionValue,
        effects: AtomicEffects,
    ) -> Self {
        Self {
            occurrence_guard,
            authority_guard,
            idempotency,
            receipt,
            entity,
            signal,
            value,
            effects,
        }
    }

    pub const fn occurrence_guard(&self) -> &OccurrenceGuard {
        &self.occurrence_guard
    }

    pub const fn authority_guard(&self) -> &SourceAuthorityGuard {
        &self.authority_guard
    }

    pub const fn idempotency(&self) -> &IdempotencyCommit {
        &self.idempotency
    }

    pub const fn receipt(&self) -> &SourceReceipt {
        &self.receipt
    }

    pub const fn entity(&self) -> Option<&SourceEntity> {
        self.entity.as_ref()
    }

    pub const fn signal(&self) -> Option<&AttentionSignal> {
        self.signal.as_ref()
    }

    pub const fn value(&self) -> &SourceIngestionValue {
        &self.value
    }

    pub const fn effects(&self) -> &AtomicEffects {
        &self.effects
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderCreateGuards {
    absent: CreateGuard,
    target: ReminderTargetGuard,
    current_fire: CurrentFireGuard,
}

impl ReminderCreateGuards {
    pub const fn new(
        absent: CreateGuard,
        target: ReminderTargetGuard,
        current_fire: CurrentFireGuard,
    ) -> Self {
        Self {
            absent,
            target,
            current_fire,
        }
    }

    pub const fn absent(&self) -> &CreateGuard {
        &self.absent
    }

    pub const fn target(&self) -> ReminderTargetGuard {
        self.target
    }

    pub const fn current_fire(&self) -> CurrentFireGuard {
        self.current_fire
    }
}

root_bundle!(
    CreateReminderBundle,
    Reminder,
    ReminderValue,
    ReminderCreateGuards
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderMutationGuards {
    revision: ExpectedRevisionGuard,
    current_fire: CurrentFireGuard,
}

impl ReminderMutationGuards {
    pub const fn new(revision: ExpectedRevisionGuard, current_fire: CurrentFireGuard) -> Self {
        Self {
            revision,
            current_fire,
        }
    }

    pub const fn revision(&self) -> &ExpectedRevisionGuard {
        &self.revision
    }

    pub const fn current_fire(&self) -> CurrentFireGuard {
        self.current_fire
    }
}

root_bundle!(
    FireReminderBundle,
    Reminder,
    ReminderValue,
    ReminderMutationGuards
);
root_bundle!(
    AcknowledgeReminderFireBundle,
    Reminder,
    ReminderValue,
    ReminderMutationGuards
);
root_bundle!(
    SnoozeReminderFireBundle,
    Reminder,
    ReminderValue,
    ReminderMutationGuards
);
