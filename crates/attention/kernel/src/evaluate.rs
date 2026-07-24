//! Pure operation-specific command evaluators.

use crate::AcknowledgeAttentionSignal;
use crate::AcknowledgeAttentionSignalBundle;
use crate::AcknowledgeReminderFire;
use crate::AcknowledgeReminderFireBundle;
use crate::AffectedView;
use crate::AtomicEffects;
use crate::AttentionSignal;
use crate::AttentionSignalValue;
use crate::CancelWorkItem;
use crate::CancelWorkItemBundle;
use crate::CanonicalCommand;
use crate::ChangeEventDraft;
use crate::ChangeEventId;
use crate::ChangeKind;
use crate::CompleteWorkItem;
use crate::CompleteWorkItemBundle;
use crate::CreateGuard;
use crate::CreateReminder;
use crate::CreateReminderBundle;
use crate::CreateWorkItem;
use crate::CreateWorkItemBundle;
use crate::CurrentFireGuard;
use crate::DeliveryPurpose;
use crate::DeliverySubject;
use crate::ExpectedRevisionGuard;
use crate::FireReminder;
use crate::FireReminderBundle;
use crate::IdempotencyCommit;
use crate::InboxEffects;
use crate::IngestSourceOccurrence;
use crate::IngestSourceOccurrenceBundle;
use crate::InvariantError;
use crate::ObservedSourceAuthority;
use crate::OccurrenceGuard;
use crate::OutboxDeduplicationKey;
use crate::OutboxIntent;
use crate::OutboxIntentId;
use crate::ReceiptOnlyReason;
use crate::Reminder;
use crate::ReminderCreateGuards;
use crate::ReminderMutationGuards;
use crate::ReminderTargetGuard;
use crate::ReminderValue;
use crate::ResourceRef;
use crate::SemanticError;
use crate::SignalAttentionState;
use crate::SnoozeReminderFire;
use crate::SnoozeReminderFireBundle;
use crate::SourceAuthorityGuard;
use crate::SourceEntity;
use crate::SourceIngestionDecision;
use crate::SourceIngestionValue;
use crate::SourceOrderComparison;
use crate::SourceOrderMode;
use crate::SourceReceipt;
use crate::SourceStateVersion;
use crate::WorkItem;
use crate::WorkItemValue;
use chrono::DateTime;
use chrono::Utc;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluationContext {
    change_event_id: ChangeEventId,
    outbox_intent_id: Option<OutboxIntentId>,
    occurred_at: DateTime<Utc>,
}

impl EvaluationContext {
    pub const fn new(
        change_event_id: ChangeEventId,
        outbox_intent_id: Option<OutboxIntentId>,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            change_event_id,
            outbox_intent_id,
            occurred_at,
        }
    }

    pub const fn change_event_id(self) -> ChangeEventId {
        self.change_event_id
    }

    pub const fn outbox_intent_id(self) -> Option<OutboxIntentId> {
        self.outbox_intent_id
    }

    pub const fn occurred_at(&self) -> &DateTime<Utc> {
        &self.occurred_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationError {
    Invariant(InvariantError),
    Semantic(SemanticError),
}

impl fmt::Display for EvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invariant(error) => error.fmt(formatter),
            Self::Semantic(error) => error.fmt(formatter),
        }
    }
}

impl Error for EvaluationError {}

impl From<InvariantError> for EvaluationError {
    fn from(value: InvariantError) -> Self {
        Self::Invariant(value)
    }
}

impl From<SemanticError> for EvaluationError {
    fn from(value: SemanticError) -> Self {
        Self::Semantic(value)
    }
}

pub fn evaluate_create_work_item(
    command: &CreateWorkItem,
    context: EvaluationContext,
) -> CreateWorkItemBundle {
    let root = WorkItem::new(
        command.id(),
        command.due_at().copied(),
        command.scheduled_at().copied(),
        command.defer_until().copied(),
        command.source_link().cloned(),
    );
    let value = WorkItemValue::new(root.id());
    let change = ChangeEventDraft::new(
        context.change_event_id,
        context.occurred_at,
        ChangeKind::WorkItemCreated,
        vec![AffectedView::WorkItem {
            id: root.id(),
            revision: root.revision(),
        }],
        InboxEffects::new(vec![crate::InboxEntry::WorkItem(root.id())], Vec::new()),
    );
    CreateWorkItemBundle::new(
        CreateGuard::absent(ResourceRef::WorkItem(root.id())),
        idempotency(command),
        root,
        value,
        AtomicEffects::new(change, None),
    )
}

pub fn evaluate_complete_work_item(
    command: &CompleteWorkItem,
    current: &WorkItem,
    context: EvaluationContext,
) -> Result<CompleteWorkItemBundle, EvaluationError> {
    require_work_item(command.id(), command.expected_revision(), current)?;
    let mut root = current.clone();
    root.complete()?;
    Ok(CompleteWorkItemBundle::new(
        ExpectedRevisionGuard::new(
            ResourceRef::WorkItem(command.id()),
            command.expected_revision(),
        ),
        idempotency(command),
        root.clone(),
        WorkItemValue::new(root.id()),
        AtomicEffects::new(
            work_item_change(
                &root,
                context,
                ChangeKind::WorkItemCompleted,
                InboxEffects::new(Vec::new(), current.inbox_entry().into_iter().collect()),
            ),
            None,
        ),
    ))
}

pub fn evaluate_cancel_work_item(
    command: &CancelWorkItem,
    current: &WorkItem,
    context: EvaluationContext,
) -> Result<CancelWorkItemBundle, EvaluationError> {
    require_work_item(command.id(), command.expected_revision(), current)?;
    let mut root = current.clone();
    root.cancel()?;
    Ok(CancelWorkItemBundle::new(
        ExpectedRevisionGuard::new(
            ResourceRef::WorkItem(command.id()),
            command.expected_revision(),
        ),
        idempotency(command),
        root.clone(),
        WorkItemValue::new(root.id()),
        AtomicEffects::new(
            work_item_change(
                &root,
                context,
                ChangeKind::WorkItemCancelled,
                InboxEffects::new(Vec::new(), current.inbox_entry().into_iter().collect()),
            ),
            None,
        ),
    ))
}

pub fn evaluate_acknowledge_attention_signal(
    command: &AcknowledgeAttentionSignal,
    current: &AttentionSignal,
    context: EvaluationContext,
) -> Result<AcknowledgeAttentionSignalBundle, EvaluationError> {
    if current.id() != command.id() {
        return Err(SemanticError::NotFound(ResourceRef::AttentionSignal(command.id())).into());
    }
    require_revision(
        ResourceRef::AttentionSignal(command.id()),
        command.expected_revision(),
        current.revision(),
    )?;
    let mut root = current.clone();
    root.acknowledge()?;
    let change = ChangeEventDraft::new(
        context.change_event_id,
        context.occurred_at,
        ChangeKind::AttentionSignalAcknowledged,
        vec![AffectedView::AttentionSignal {
            id: root.id(),
            revision: root.revision(),
        }],
        InboxEffects::new(Vec::new(), current.inbox_entry().into_iter().collect()),
    );
    Ok(AcknowledgeAttentionSignalBundle::new(
        ExpectedRevisionGuard::new(
            ResourceRef::AttentionSignal(command.id()),
            command.expected_revision(),
        ),
        idempotency(command),
        root,
        AttentionSignalValue::new(command.id()),
        AtomicEffects::new(change, None),
    ))
}

pub fn evaluate_ingest_source_occurrence(
    command: &IngestSourceOccurrence,
    current_entity: Option<&SourceEntity>,
    current_signal: Option<&AttentionSignal>,
    context: EvaluationContext,
) -> Result<IngestSourceOccurrenceBundle, EvaluationError> {
    let fingerprint = command.canonical_fingerprint();
    let observed = current_entity.map_or(ObservedSourceAuthority::Absent, |entity| {
        entity.observed_authority()
    });
    let decision = source_decision(command.order(), current_entity.map(SourceEntity::order));
    if decision == SourceIngestionDecision::Advanced
        && command.fresh_attention()
        && context.outbox_intent_id.is_none()
    {
        return Err(InvariantError::MissingOutboxIntentId.into());
    }
    let receipt = SourceReceipt::reconstruct(
        command.receipt_id(),
        command.occurrence_key().clone(),
        command.entity().map(|entity| entity.key().clone()),
        fingerprint,
        command.order().clone(),
        *command.occurred_at(),
        *command.ingested_at(),
    )?;

    let (entity, signal, inbox_effects, affected_views, intent) = match decision {
        SourceIngestionDecision::ReceiptOnly(_) => {
            (None, None, InboxEffects::default(), Vec::new(), None)
        }
        SourceIngestionDecision::Advanced => {
            let entity = next_entity(command, current_entity)?;
            let entity_id = entity.as_ref().map(SourceEntity::id);
            let signal = if let Some(current) = current_signal {
                if current.id() != command.signal_id() {
                    return Err(SemanticError::NotFound(ResourceRef::AttentionSignal(
                        command.signal_id(),
                    ))
                    .into());
                }
                let mut signal = current.clone();
                signal.advance_source(
                    command.receipt_id(),
                    entity_id,
                    command.source_lifecycle(),
                    command.fresh_attention(),
                )?;
                signal
            } else {
                AttentionSignal::reconstruct(
                    command.signal_id(),
                    crate::Revision::initial(),
                    command.source_lifecycle(),
                    SignalAttentionState::Unread,
                    command.receipt_id(),
                    entity_id,
                )?
            };
            let before = current_signal.and_then(AttentionSignal::inbox_entry);
            let after = signal.inbox_entry();
            let inbox_effects = inbox_delta(before, after);
            let affected_views = vec![AffectedView::AttentionSignal {
                id: signal.id(),
                revision: signal.revision(),
            }];
            let intent = if command.fresh_attention() {
                context.outbox_intent_id.map(|id| {
                    fresh_attention_intent(
                        id,
                        context.change_event_id,
                        &context.occurred_at,
                        &signal,
                    )
                })
            } else {
                None
            };
            (entity, Some(signal), inbox_effects, affected_views, intent)
        }
    };

    let change = ChangeEventDraft::new(
        context.change_event_id,
        context.occurred_at,
        ChangeKind::SourceOccurrenceIngested,
        affected_views,
        inbox_effects,
    );
    Ok(IngestSourceOccurrenceBundle::new(
        OccurrenceGuard::new(command.occurrence_key().clone(), fingerprint),
        SourceAuthorityGuard::new(
            command.entity().map(|entity| entity.key().clone()),
            observed,
        ),
        idempotency(command),
        receipt,
        entity,
        signal,
        SourceIngestionValue::new(command.receipt_id(), command.signal_id(), decision),
        AtomicEffects::new(change, intent),
    ))
}

pub fn evaluate_create_reminder(
    command: &CreateReminder,
    context: EvaluationContext,
) -> CreateReminderBundle {
    let root = Reminder::new(
        command.reminder_id(),
        command.target(),
        *command.trigger_at(),
        command.initial_fire_id(),
    );
    let change = reminder_change(
        &root,
        command.initial_fire_id(),
        context,
        ChangeKind::ReminderCreated,
        InboxEffects::default(),
    );
    CreateReminderBundle::new(
        ReminderCreateGuards::new(
            CreateGuard::absent(ResourceRef::Reminder(command.reminder_id())),
            ReminderTargetGuard::unique(command.target()),
            CurrentFireGuard::new(command.initial_fire_id()),
        ),
        idempotency(command),
        root,
        ReminderValue::new(command.reminder_id(), command.initial_fire_id()),
        AtomicEffects::new(change, None),
    )
}

pub fn evaluate_fire_reminder(
    command: &FireReminder,
    current: &Reminder,
    context: EvaluationContext,
) -> Result<FireReminderBundle, EvaluationError> {
    require_reminder(command.reminder_id(), current)?;
    let observed_revision = current.revision();
    let mut root = current.clone();
    root.mark_fired(command.fire_id())?;
    let intent = context.outbox_intent_id.map(|id| {
        reminder_intent(
            id,
            context.change_event_id,
            &context.occurred_at,
            command.fire_id(),
        )
    });
    Ok(FireReminderBundle::new(
        reminder_guards(command.reminder_id(), command.fire_id(), observed_revision),
        idempotency(command),
        root.clone(),
        ReminderValue::new(command.reminder_id(), command.fire_id()),
        AtomicEffects::new(
            reminder_change(
                &root,
                command.fire_id(),
                context,
                ChangeKind::ReminderFired,
                InboxEffects::new(
                    vec![crate::InboxEntry::ReminderFire(command.fire_id())],
                    Vec::new(),
                ),
            ),
            intent,
        ),
    ))
}

pub fn evaluate_acknowledge_reminder_fire(
    command: &AcknowledgeReminderFire,
    current: &Reminder,
    context: EvaluationContext,
) -> Result<AcknowledgeReminderFireBundle, EvaluationError> {
    require_reminder(command.reminder_id(), current)?;
    require_revision(
        ResourceRef::Reminder(command.reminder_id()),
        command.expected_revision(),
        current.revision(),
    )?;
    let mut root = current.clone();
    root.acknowledge_fire(command.fire_id())?;
    Ok(AcknowledgeReminderFireBundle::new(
        reminder_guards(
            command.reminder_id(),
            command.fire_id(),
            command.expected_revision(),
        ),
        idempotency(command),
        root.clone(),
        ReminderValue::new(command.reminder_id(), command.fire_id()),
        AtomicEffects::new(
            reminder_change(
                &root,
                command.fire_id(),
                context,
                ChangeKind::ReminderFireAcknowledged,
                InboxEffects::new(
                    Vec::new(),
                    vec![crate::InboxEntry::ReminderFire(command.fire_id())],
                ),
            ),
            None,
        ),
    ))
}

pub fn evaluate_snooze_reminder_fire(
    command: &SnoozeReminderFire,
    current: &Reminder,
    context: EvaluationContext,
) -> Result<SnoozeReminderFireBundle, EvaluationError> {
    require_reminder(command.reminder_id(), current)?;
    require_revision(
        ResourceRef::Reminder(command.reminder_id()),
        command.expected_revision(),
        current.revision(),
    )?;
    let mut root = current.clone();
    root.snooze_fire(
        command.fire_id(),
        command.replacement_fire_id(),
        *command.replacement_trigger_at(),
    )?;
    Ok(SnoozeReminderFireBundle::new(
        reminder_guards(
            command.reminder_id(),
            command.fire_id(),
            command.expected_revision(),
        ),
        idempotency(command),
        root.clone(),
        ReminderValue::new(command.reminder_id(), command.replacement_fire_id()),
        AtomicEffects::new(
            reminder_change(
                &root,
                command.replacement_fire_id(),
                context,
                ChangeKind::ReminderFireSnoozed,
                InboxEffects::new(
                    Vec::new(),
                    vec![crate::InboxEntry::ReminderFire(command.fire_id())],
                ),
            ),
            None,
        ),
    ))
}

fn idempotency<C: CanonicalCommand>(command: &C) -> IdempotencyCommit {
    IdempotencyCommit::new(
        command.idempotency_key(),
        command.operation(),
        command.canonical_fingerprint(),
    )
}

fn require_work_item(
    id: crate::WorkItemId,
    expected: crate::Revision,
    current: &WorkItem,
) -> Result<(), SemanticError> {
    if current.id() != id {
        return Err(SemanticError::NotFound(ResourceRef::WorkItem(id)));
    }
    require_revision(ResourceRef::WorkItem(id), expected, current.revision())
}

fn require_reminder(id: crate::ReminderId, current: &Reminder) -> Result<(), SemanticError> {
    if current.id() != id {
        return Err(SemanticError::NotFound(ResourceRef::Reminder(id)));
    }
    Ok(())
}

fn require_revision(
    resource: ResourceRef,
    expected: crate::Revision,
    actual: crate::Revision,
) -> Result<(), SemanticError> {
    if expected != actual {
        return Err(SemanticError::ExpectedRevisionConflict {
            resource,
            expected,
            actual,
        });
    }
    Ok(())
}

fn work_item_change(
    root: &WorkItem,
    context: EvaluationContext,
    kind: ChangeKind,
    effects: InboxEffects,
) -> ChangeEventDraft {
    ChangeEventDraft::new(
        context.change_event_id,
        context.occurred_at,
        kind,
        vec![AffectedView::WorkItem {
            id: root.id(),
            revision: root.revision(),
        }],
        effects,
    )
}

fn reminder_change(
    root: &Reminder,
    fire_id: crate::ReminderFireId,
    context: EvaluationContext,
    kind: ChangeKind,
    effects: InboxEffects,
) -> ChangeEventDraft {
    ChangeEventDraft::new(
        context.change_event_id,
        context.occurred_at,
        kind,
        vec![
            AffectedView::Reminder {
                id: root.id(),
                revision: root.revision(),
            },
            AffectedView::ReminderFire {
                reminder_id: root.id(),
                fire_id,
                reminder_revision: root.revision(),
            },
        ],
        effects,
    )
}

fn source_decision(
    proposed: &SourceOrderMode,
    current: Option<&SourceOrderMode>,
) -> SourceIngestionDecision {
    let Some(current) = current else {
        return SourceIngestionDecision::Advanced;
    };
    match (proposed, current) {
        (SourceOrderMode::Unordered, SourceOrderMode::Unordered) => {
            SourceIngestionDecision::Advanced
        }
        (SourceOrderMode::Ordered { value: None, .. }, SourceOrderMode::Ordered { .. }) => {
            SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::MissingOrderedValue)
        }
        (
            SourceOrderMode::Ordered {
                domain: proposed, ..
            },
            SourceOrderMode::Ordered {
                domain: current, ..
            },
        ) if proposed != current => {
            SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::ComparatorDomainMismatch)
        }
        (SourceOrderMode::Ordered { .. }, SourceOrderMode::Ordered { .. }) => {
            match proposed.compare_to(current) {
                SourceOrderComparison::Newer => SourceIngestionDecision::Advanced,
                SourceOrderComparison::Equal => {
                    SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Equal)
                }
                SourceOrderComparison::Older => {
                    SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Older)
                }
                SourceOrderComparison::Incomparable => {
                    SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Incomparable)
                }
            }
        }
        _ => SourceIngestionDecision::ReceiptOnly(ReceiptOnlyReason::Incomparable),
    }
}

fn next_entity(
    command: &IngestSourceOccurrence,
    current: Option<&SourceEntity>,
) -> Result<Option<SourceEntity>, InvariantError> {
    let Some(identity) = command.entity() else {
        return Ok(None);
    };
    current.map_or_else(
        || {
            SourceEntity::reconstruct(
                identity.id(),
                identity.key().clone(),
                SourceStateVersion::initial(),
                command.receipt_id(),
                command.order().clone(),
            )
            .map(Some)
        },
        |entity| {
            entity
                .advance(command.receipt_id(), command.order().clone())
                .map(Some)
        },
    )
}

fn inbox_delta(
    before: Option<crate::InboxEntry>,
    after: Option<crate::InboxEntry>,
) -> InboxEffects {
    if before == after {
        return InboxEffects::default();
    }
    InboxEffects::new(after.into_iter().collect(), before.into_iter().collect())
}

fn fresh_attention_intent(
    id: OutboxIntentId,
    event_id: ChangeEventId,
    occurred_at: &DateTime<Utc>,
    signal: &AttentionSignal,
) -> OutboxIntent {
    let key = OutboxDeduplicationKey::for_attention_signal(signal.id());
    OutboxIntent::new(
        id,
        key,
        DeliverySubject::AttentionSignal(signal.id()),
        event_id,
        *occurred_at,
        DeliveryPurpose::FreshAttention,
    )
}

fn reminder_intent(
    id: OutboxIntentId,
    event_id: ChangeEventId,
    occurred_at: &DateTime<Utc>,
    fire_id: crate::ReminderFireId,
) -> OutboxIntent {
    let key = OutboxDeduplicationKey::for_reminder_fire(fire_id);
    OutboxIntent::new(
        id,
        key,
        DeliverySubject::ReminderFire(fire_id),
        event_id,
        *occurred_at,
        DeliveryPurpose::ReminderFired,
    )
}

fn reminder_guards(
    reminder_id: crate::ReminderId,
    fire_id: crate::ReminderFireId,
    revision: crate::Revision,
) -> ReminderMutationGuards {
    ReminderMutationGuards::new(
        ExpectedRevisionGuard::new(ResourceRef::Reminder(reminder_id), revision),
        CurrentFireGuard::new(fire_id),
    )
}
