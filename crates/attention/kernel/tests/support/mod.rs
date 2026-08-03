#![expect(
    dead_code,
    reason = "shared support surface varies by integration target"
)]
#![expect(
    clippy::expect_used,
    reason = "test-only deterministic in-memory adapter"
)]
use attention_kernel::*;
use chrono::DateTime;
use chrono::Utc;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone)]
pub struct MemoryAdapter {
    state: Arc<Mutex<State>>,
}

struct State {
    cursor: u64,
    work_items: HashMap<WorkItemId, WorkItem>,
    signals: HashMap<AttentionSignalId, AttentionSignal>,
    reminders: HashMap<ReminderId, Reminder>,
    source_entities: HashMap<SourceEntityKey, SourceEntity>,
    receipts: HashMap<OccurrenceKey, SourceReceipt>,
    receipt_occurrences: HashMap<SourceReceiptId, OccurrenceKey>,
    receipt_outcomes: HashMap<OccurrenceKey, IngestSourceOccurrenceResult>,
    idempotency: HashMap<MutationIdempotencyKey, StoredOutcome>,
    events: Vec<ChangeEvent>,
    inbox: HashSet<InboxEntry>,
    outbox: HashMap<OutboxIntentId, OutboxIntent>,
    deliveries: HashMap<OutboxIntentId, DeliveryState>,
    checkpoints: HashMap<String, CommitCursor>,
    token_counter: u64,
    retention_floor: Option<CommitCursor>,
}

#[derive(Clone)]
struct StoredOutcome {
    operation: MutationOperation,
    fingerprint: CanonicalFingerprint,
    outcome: PriorMutationOutcome,
}

impl Default for State {
    fn default() -> Self {
        Self {
            cursor: 1,
            work_items: HashMap::new(),
            signals: HashMap::new(),
            reminders: HashMap::new(),
            source_entities: HashMap::new(),
            receipts: HashMap::new(),
            receipt_occurrences: HashMap::new(),
            receipt_outcomes: HashMap::new(),
            idempotency: HashMap::new(),
            events: Vec::new(),
            inbox: HashSet::new(),
            outbox: HashMap::new(),
            deliveries: HashMap::new(),
            checkpoints: HashMap::new(),
            token_counter: 0,
            retention_floor: None,
        }
    }
}

impl MemoryAdapter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    pub fn event_count(&self) -> usize {
        self.state.lock().expect("state lock").events.len()
    }

    pub fn outbox_count(&self) -> usize {
        self.state.lock().expect("state lock").outbox.len()
    }

    pub fn inbox(&self) -> HashSet<InboxEntry> {
        self.state.lock().expect("state lock").inbox.clone()
    }

    pub fn receipt_for_occurrence(&self, key: &OccurrenceKey) -> Option<SourceReceipt> {
        self.state
            .lock()
            .expect("state lock")
            .receipts
            .get(key)
            .cloned()
    }

    pub fn set_retention_floor(&self, floor: CommitCursor) {
        self.state.lock().expect("state lock").retention_floor = Some(floor);
    }

    pub fn insert_delivery(&self, intent: OutboxIntent) {
        let mut state = self.state.lock().expect("state lock");
        state
            .deliveries
            .insert(intent.id(), DeliveryState::pending(intent.id()));
        state.outbox.insert(intent.id(), intent);
    }
}

fn semantic<T>(error: SemanticError) -> Result<T, PortError<Infallible>> {
    Err(PortError::Semantic(error))
}

fn check_replay(
    state: &State,
    idempotency: &IdempotencyCommit,
) -> Result<Option<PriorMutationOutcome>, PortError<Infallible>> {
    let Some(stored) = state.idempotency.get(&idempotency.key()) else {
        return Ok(None);
    };
    if stored.operation != idempotency.operation()
        || stored.fingerprint != idempotency.fingerprint()
    {
        return semantic(SemanticError::IdempotencyMismatch(idempotency.key()));
    }
    Ok(Some(stored.outcome.replayed()))
}

fn store_outcome(
    state: &mut State,
    idempotency: &IdempotencyCommit,
    outcome: PriorMutationOutcome,
) {
    state.idempotency.insert(
        idempotency.key(),
        StoredOutcome {
            operation: idempotency.operation(),
            fingerprint: idempotency.fingerprint(),
            outcome,
        },
    );
}

fn apply_effects(state: &mut State, effects: &AtomicEffects) -> (CommitCursor, ChangeEventId) {
    state.cursor += 1;
    let cursor = CommitCursor::try_from(state.cursor).expect("nonzero test cursor");
    for entry in effects.change().inbox_effects().removals() {
        state.inbox.remove(entry);
    }
    for entry in effects.change().inbox_effects().additions() {
        state.inbox.insert(*entry);
    }
    let event_id = effects.change().id();
    state.events.push(effects.change().clone().commit(cursor));
    if let Some(intent) = effects.outbox_intent() {
        state
            .deliveries
            .insert(intent.id(), DeliveryState::pending(intent.id()));
        state.outbox.insert(intent.id(), intent.clone());
    }
    (cursor, event_id)
}

fn outcome<T: Clone>(
    value: &T,
    effects: &AtomicEffects,
    cursor: CommitCursor,
    event_id: ChangeEventId,
) -> CommandOutcome<T> {
    CommandOutcome::new(
        CommandDisposition::Applied,
        value.clone(),
        cursor,
        event_id,
        effects.outbox_intent().map(OutboxIntent::id),
    )
}

fn revision_conflict(
    resource: ResourceRef,
    expected: Revision,
    actual: Revision,
) -> PortError<Infallible> {
    PortError::Semantic(SemanticError::ExpectedRevisionConflict {
        resource,
        expected,
        actual,
    })
}

impl AttentionReadPort for MemoryAdapter {
    type Error = Infallible;

    fn work_item(
        &self,
        id: WorkItemId,
    ) -> BoxFuture<'_, Result<Option<WorkItem>, PortError<Self::Error>>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("state lock")
                .work_items
                .get(&id)
                .cloned())
        })
    }

    fn attention_signal(
        &self,
        id: AttentionSignalId,
    ) -> BoxFuture<'_, Result<Option<AttentionSignal>, PortError<Self::Error>>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("state lock")
                .signals
                .get(&id)
                .cloned())
        })
    }

    fn reminder(
        &self,
        id: ReminderId,
    ) -> BoxFuture<'_, Result<Option<Reminder>, PortError<Self::Error>>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("state lock")
                .reminders
                .get(&id)
                .cloned())
        })
    }

    fn source_entity(
        &self,
        query: SourceAuthorityQuery,
    ) -> BoxFuture<'_, Result<Option<SourceEntity>, PortError<Self::Error>>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("state lock")
                .source_entities
                .get(query.key())
                .cloned())
        })
    }

    fn source_receipt(
        &self,
        id: SourceReceiptId,
    ) -> BoxFuture<'_, Result<Option<SourceReceipt>, PortError<Self::Error>>> {
        Box::pin(async move {
            let state = self.state.lock().expect("state lock");
            Ok(state
                .receipt_occurrences
                .get(&id)
                .and_then(|key| state.receipts.get(key))
                .cloned())
        })
    }

    fn prior_outcome(
        &self,
        query: PriorOutcomeQuery,
    ) -> BoxFuture<'_, Result<Option<PriorMutationOutcome>, PortError<Self::Error>>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("state lock")
                .idempotency
                .get(&query.key())
                .map(|stored| stored.outcome.clone()))
        })
    }

    fn snapshot(&self) -> BoxFuture<'_, Result<SnapshotResult, PortError<Self::Error>>> {
        Box::pin(async move {
            let state = self.state.lock().expect("state lock");
            Ok(AttentionSnapshot::new(
                CommitCursor::try_from(state.cursor).expect("nonzero cursor"),
                state.work_items.values().cloned().collect(),
                state.signals.values().cloned().collect(),
                state.reminders.values().cloned().collect(),
            ))
        })
    }

    fn changes_after(
        &self,
        query: ChangesAfterQuery,
    ) -> BoxFuture<'_, Result<ChangesAfterResult, PortError<Self::Error>>> {
        Box::pin(async move {
            let state = self.state.lock().expect("state lock");
            let head = CommitCursor::try_from(state.cursor).expect("nonzero cursor");
            let floor = state
                .retention_floor
                .unwrap_or_else(|| CommitCursor::try_from(1).expect("nonzero genesis cursor"));
            if query.after() < floor {
                return Ok(ChangesResult::Gap(ChangeGap::Expired {
                    requested_after: query.after(),
                    earliest_available: floor,
                    latest_available: head,
                }));
            }
            if query.after() > head {
                return Ok(ChangesResult::Gap(ChangeGap::Future {
                    requested_after: query.after(),
                    latest_available: head,
                }));
            }
            let mut matching: Vec<_> = state
                .events
                .iter()
                .filter(|event| event.cursor() > query.after() && event.cursor() <= head)
                .cloned()
                .collect();
            let has_more = matching.len() > query.limit().value();
            matching.truncate(query.limit().value());
            let resume = matching
                .last()
                .map_or_else(|| query.after(), ChangeEvent::cursor);
            Ok(ChangesResult::Page(ChangePage::new(
                matching, resume, has_more,
            )))
        })
    }
}

impl AttentionCommitPort for MemoryAdapter {
    type Error = Infallible;

    fn commit_create_work_item(
        &self,
        bundle: CreateWorkItemBundle,
    ) -> BoxFuture<'_, Result<CreateWorkItemResult, PortError<Self::Error>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            if let Some(PriorMutationOutcome::CreateWorkItem(stored)) =
                check_replay(&state, bundle.idempotency())?
            {
                return Ok(stored);
            }
            if state.work_items.contains_key(&bundle.root().id()) {
                return semantic(SemanticError::CreateConflict(ResourceRef::WorkItem(
                    bundle.root().id(),
                )));
            }
            let (cursor, event_id) = apply_effects(&mut state, bundle.effects());
            state
                .work_items
                .insert(bundle.root().id(), bundle.root().clone());
            let result = outcome(bundle.value(), bundle.effects(), cursor, event_id);
            store_outcome(
                &mut state,
                bundle.idempotency(),
                PriorMutationOutcome::CreateWorkItem(result.clone()),
            );
            Ok(result)
        })
    }

    fn commit_complete_work_item(
        &self,
        bundle: CompleteWorkItemBundle,
    ) -> BoxFuture<'_, Result<CompleteWorkItemResult, PortError<Self::Error>>> {
        self.commit_work_item_mutation(bundle)
    }

    fn commit_cancel_work_item(
        &self,
        bundle: CancelWorkItemBundle,
    ) -> BoxFuture<'_, Result<CancelWorkItemResult, PortError<Self::Error>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            if let Some(PriorMutationOutcome::CancelWorkItem(stored)) =
                check_replay(&state, bundle.idempotency())?
            {
                return Ok(stored);
            }
            check_work_item_revision(&state, bundle.guard())?;
            let (cursor, event_id) = apply_effects(&mut state, bundle.effects());
            state
                .work_items
                .insert(bundle.root().id(), bundle.root().clone());
            let result = outcome(bundle.value(), bundle.effects(), cursor, event_id);
            store_outcome(
                &mut state,
                bundle.idempotency(),
                PriorMutationOutcome::CancelWorkItem(result.clone()),
            );
            Ok(result)
        })
    }

    fn commit_acknowledge_attention_signal(
        &self,
        bundle: AcknowledgeAttentionSignalBundle,
    ) -> BoxFuture<'_, Result<AcknowledgeAttentionSignalResult, PortError<Self::Error>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            if let Some(PriorMutationOutcome::AcknowledgeAttentionSignal(stored)) =
                check_replay(&state, bundle.idempotency())?
            {
                return Ok(stored);
            }
            let ResourceRef::AttentionSignal(id) = bundle.guard().resource() else {
                return semantic(SemanticError::NotFound(bundle.guard().resource().clone()));
            };
            let Some(current) = state.signals.get(id) else {
                return semantic(SemanticError::NotFound(ResourceRef::AttentionSignal(*id)));
            };
            if current.revision() != bundle.guard().expected() {
                return Err(revision_conflict(
                    ResourceRef::AttentionSignal(*id),
                    bundle.guard().expected(),
                    current.revision(),
                ));
            }
            let (cursor, event_id) = apply_effects(&mut state, bundle.effects());
            state
                .signals
                .insert(bundle.root().id(), bundle.root().clone());
            let result = outcome(bundle.value(), bundle.effects(), cursor, event_id);
            store_outcome(
                &mut state,
                bundle.idempotency(),
                PriorMutationOutcome::AcknowledgeAttentionSignal(result.clone()),
            );
            Ok(result)
        })
    }

    fn commit_ingest_source_occurrence(
        &self,
        bundle: IngestSourceOccurrenceBundle,
    ) -> BoxFuture<'_, Result<IngestSourceOccurrenceResult, PortError<Self::Error>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            if let Some(PriorMutationOutcome::IngestSourceOccurrence(stored)) =
                check_replay(&state, bundle.idempotency())?
            {
                return Ok(stored);
            }
            if let Some(existing) = state.receipts.get(bundle.occurrence_guard().key()) {
                if existing.fingerprint() != bundle.occurrence_guard().fingerprint() {
                    return semantic(SemanticError::OccurrenceContentMismatch(
                        bundle.occurrence_guard().key().clone(),
                    ));
                }
                if let Some(stored) = state.receipt_outcomes.get(bundle.occurrence_guard().key()) {
                    return Ok(stored.replayed());
                }
                return semantic(SemanticError::CreateConflict(
                    ResourceRef::SourceOccurrence(bundle.occurrence_guard().key().clone()),
                ));
            }
            check_source_authority(&state, bundle.authority_guard())?;
            if let Some(existing_occurrence) = state.receipt_occurrences.get(&bundle.receipt().id())
                && existing_occurrence != bundle.occurrence_guard().key()
            {
                let receipt_id = bundle.receipt().id();
                let existing_occurrence = existing_occurrence.clone();
                let proposed_occurrence = bundle.occurrence_guard().key().clone();
                drop(state);
                panic!(
                    "source receipt ID index collision: {receipt_id} maps to {existing_occurrence:?}, attempted {proposed_occurrence:?}"
                );
            }
            let (cursor, event_id) = apply_effects(&mut state, bundle.effects());
            state.receipts.insert(
                bundle.receipt().occurrence_key().clone(),
                bundle.receipt().clone(),
            );
            state.receipt_occurrences.insert(
                bundle.receipt().id(),
                bundle.receipt().occurrence_key().clone(),
            );
            if let Some(entity) = bundle.entity() {
                state
                    .source_entities
                    .insert(entity.key().clone(), entity.clone());
            }
            if let Some(signal) = bundle.signal() {
                state.signals.insert(signal.id(), signal.clone());
            }
            let result = outcome(bundle.value(), bundle.effects(), cursor, event_id);
            state
                .receipt_outcomes
                .insert(bundle.occurrence_guard().key().clone(), result.clone());
            store_outcome(
                &mut state,
                bundle.idempotency(),
                PriorMutationOutcome::IngestSourceOccurrence(result.clone()),
            );
            Ok(result)
        })
    }

    fn commit_create_reminder(
        &self,
        bundle: CreateReminderBundle,
    ) -> BoxFuture<'_, Result<CreateReminderResult, PortError<Self::Error>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            if let Some(PriorMutationOutcome::CreateReminder(stored)) =
                check_replay(&state, bundle.idempotency())?
            {
                return Ok(stored);
            }
            if state.reminders.contains_key(&bundle.root().id())
                || state
                    .reminders
                    .values()
                    .any(|reminder| reminder.target() == bundle.root().target())
            {
                return semantic(SemanticError::CreateConflict(ResourceRef::Reminder(
                    bundle.root().id(),
                )));
            }
            let (cursor, event_id) = apply_effects(&mut state, bundle.effects());
            state
                .reminders
                .insert(bundle.root().id(), bundle.root().clone());
            let result = outcome(bundle.value(), bundle.effects(), cursor, event_id);
            store_outcome(
                &mut state,
                bundle.idempotency(),
                PriorMutationOutcome::CreateReminder(result.clone()),
            );
            Ok(result)
        })
    }

    fn commit_fire_reminder(
        &self,
        bundle: FireReminderBundle,
    ) -> BoxFuture<'_, Result<FireReminderResult, PortError<Self::Error>>> {
        self.commit_reminder_fire(bundle)
    }

    fn commit_acknowledge_reminder_fire(
        &self,
        bundle: AcknowledgeReminderFireBundle,
    ) -> BoxFuture<'_, Result<AcknowledgeReminderFireResult, PortError<Self::Error>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            if let Some(PriorMutationOutcome::AcknowledgeReminderFire(stored)) =
                check_replay(&state, bundle.idempotency())?
            {
                return Ok(stored);
            }
            check_reminder_revision(&state, bundle.guard())?;
            let (cursor, event_id) = apply_effects(&mut state, bundle.effects());
            state
                .reminders
                .insert(bundle.root().id(), bundle.root().clone());
            let result = outcome(bundle.value(), bundle.effects(), cursor, event_id);
            store_outcome(
                &mut state,
                bundle.idempotency(),
                PriorMutationOutcome::AcknowledgeReminderFire(result.clone()),
            );
            Ok(result)
        })
    }

    fn commit_snooze_reminder_fire(
        &self,
        bundle: SnoozeReminderFireBundle,
    ) -> BoxFuture<'_, Result<SnoozeReminderFireResult, PortError<Self::Error>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            if let Some(PriorMutationOutcome::SnoozeReminderFire(stored)) =
                check_replay(&state, bundle.idempotency())?
            {
                return Ok(stored);
            }
            check_reminder_revision(&state, bundle.guard())?;
            let (cursor, event_id) = apply_effects(&mut state, bundle.effects());
            state
                .reminders
                .insert(bundle.root().id(), bundle.root().clone());
            let result = outcome(bundle.value(), bundle.effects(), cursor, event_id);
            store_outcome(
                &mut state,
                bundle.idempotency(),
                PriorMutationOutcome::SnoozeReminderFire(result.clone()),
            );
            Ok(result)
        })
    }
}

impl MemoryAdapter {
    fn commit_work_item_mutation(
        &self,
        bundle: CompleteWorkItemBundle,
    ) -> BoxFuture<'_, Result<CompleteWorkItemResult, PortError<Infallible>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            if let Some(PriorMutationOutcome::CompleteWorkItem(stored)) =
                check_replay(&state, bundle.idempotency())?
            {
                return Ok(stored);
            }
            check_work_item_revision(&state, bundle.guard())?;
            let (cursor, event_id) = apply_effects(&mut state, bundle.effects());
            state
                .work_items
                .insert(bundle.root().id(), bundle.root().clone());
            let result = outcome(bundle.value(), bundle.effects(), cursor, event_id);
            store_outcome(
                &mut state,
                bundle.idempotency(),
                PriorMutationOutcome::CompleteWorkItem(result.clone()),
            );
            Ok(result)
        })
    }

    fn commit_reminder_fire(
        &self,
        bundle: FireReminderBundle,
    ) -> BoxFuture<'_, Result<FireReminderResult, PortError<Infallible>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            if let Some(PriorMutationOutcome::FireReminder(stored)) =
                check_replay(&state, bundle.idempotency())?
            {
                return Ok(stored);
            }
            check_reminder_revision(&state, bundle.guard())?;
            let (cursor, event_id) = apply_effects(&mut state, bundle.effects());
            state
                .reminders
                .insert(bundle.root().id(), bundle.root().clone());
            let result = outcome(bundle.value(), bundle.effects(), cursor, event_id);
            store_outcome(
                &mut state,
                bundle.idempotency(),
                PriorMutationOutcome::FireReminder(result.clone()),
            );
            Ok(result)
        })
    }
}

fn check_work_item_revision(
    state: &State,
    guard: &ExpectedRevisionGuard,
) -> Result<(), PortError<Infallible>> {
    let ResourceRef::WorkItem(id) = guard.resource() else {
        return semantic(SemanticError::NotFound(guard.resource().clone()));
    };
    let Some(current) = state.work_items.get(id) else {
        return semantic(SemanticError::NotFound(ResourceRef::WorkItem(*id)));
    };
    if current.revision() != guard.expected() {
        return Err(revision_conflict(
            ResourceRef::WorkItem(*id),
            guard.expected(),
            current.revision(),
        ));
    }
    Ok(())
}

fn check_source_authority(
    state: &State,
    guard: &SourceAuthorityGuard,
) -> Result<(), PortError<Infallible>> {
    let actual = guard.key().and_then(|key| state.source_entities.get(key));
    let actual_authority = actual.map_or(
        ObservedSourceAuthority::Absent,
        SourceEntity::observed_authority,
    );
    if &actual_authority != guard.observed() {
        let key = guard
            .key()
            .cloned()
            .expect("versioned source guard has key");
        return semantic(SemanticError::ObservedSourceVersionConflict {
            entity: key,
            observed: guard.observed().version(),
            actual: actual_authority.version(),
        });
    }
    Ok(())
}

fn check_reminder_revision(
    state: &State,
    guard: &ReminderMutationGuards,
) -> Result<(), PortError<Infallible>> {
    let ResourceRef::Reminder(id) = guard.revision().resource() else {
        return semantic(SemanticError::NotFound(guard.revision().resource().clone()));
    };
    let Some(current) = state.reminders.get(id) else {
        return semantic(SemanticError::NotFound(ResourceRef::Reminder(*id)));
    };
    if current.revision() != guard.revision().expected() {
        return Err(revision_conflict(
            ResourceRef::Reminder(*id),
            guard.revision().expected(),
            current.revision(),
        ));
    }
    Ok(())
}

impl ReminderSchedulePort for MemoryAdapter {
    type Error = Infallible;

    fn due_reminder_fires(
        &self,
        query: DueReminderFiresQuery,
    ) -> BoxFuture<'_, Result<Vec<DueReminderFire>, PortError<Self::Error>>> {
        Box::pin(async move {
            let state = self.state.lock().expect("state lock");
            let mut due: Vec<_> = state
                .reminders
                .values()
                .flat_map(|reminder| {
                    reminder
                        .fires()
                        .iter()
                        .filter(|fire| {
                            fire.state() == ReminderFireState::Scheduled
                                && fire.trigger_at() <= query.due_at_or_before()
                        })
                        .map(|fire| {
                            DueReminderFire::new(
                                reminder.id(),
                                fire.id(),
                                reminder.revision(),
                                *fire.trigger_at(),
                            )
                        })
                })
                .collect();
            due.sort_by_key(|fire| (*fire.trigger_at(), fire.fire_id(), fire.reminder_id()));
            due.truncate(query.limit().value());
            Ok(due)
        })
    }
}

impl DeliveryPort for MemoryAdapter {
    type Error = Infallible;

    fn claim(
        &self,
        query: DeliveryClaimQuery,
    ) -> BoxFuture<'_, Result<Vec<DeliveryClaim>, PortError<Self::Error>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            let mut eligible: Vec<_> = state
                .deliveries
                .iter()
                .filter_map(|(id, delivery)| {
                    let is_eligible = match delivery.status() {
                        DeliveryStatus::Pending => true,
                        DeliveryStatus::Retryable { next_retry_at, .. }
                            if next_retry_at <= query.eligible_at() =>
                        {
                            true
                        }
                        DeliveryStatus::Leased { expires_at, .. }
                            if expires_at <= query.eligible_at() =>
                        {
                            true
                        }
                        _ => false,
                    };
                    if is_eligible {
                        let intent = state.outbox.get(id).expect("delivery has outbox intent");
                        Some((*intent.created_at(), *id))
                    } else {
                        None
                    }
                })
                .collect();
            eligible.sort_unstable();
            eligible.truncate(query.limit().value());
            let mut claims = Vec::new();
            for (_, id) in eligible {
                state.token_counter += 1;
                let mut bytes = [0; 32];
                bytes[24..].copy_from_slice(&state.token_counter.to_be_bytes());
                let token = DeliveryLeaseToken::from_bytes(bytes);
                let delivery = state.deliveries.get_mut(&id).expect("selected delivery");
                if let ClaimOutcome::Claimed(claim) =
                    delivery.claim(token, *query.lease_expires_at())
                {
                    claims.push(claim);
                }
            }
            Ok(claims)
        })
    }

    fn inspect(
        &self,
        intent_id: OutboxIntentId,
    ) -> BoxFuture<'_, Result<Option<DeliveryAuthority>, PortError<Self::Error>>> {
        Box::pin(async move {
            let state = self.state.lock().expect("state lock");
            Ok(state.outbox.get(&intent_id).and_then(|intent| {
                state
                    .deliveries
                    .get(&intent_id)
                    .map(|delivery| DeliveryAuthority::new(intent.clone(), delivery.clone()))
            }))
        })
    }

    fn renew(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        expires_at: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<RenewOutcome, PortError<Self::Error>>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("state lock")
                .deliveries
                .get_mut(&intent_id)
                .map_or(RenewOutcome::NotLeased, |state| {
                    state.renew(token, expires_at)
                }))
        })
    }

    fn succeed(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        provider_message_id: ProviderMessageId,
        succeeded_at: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<DeliveryCompletionOutcome, PortError<Self::Error>>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("state lock")
                .deliveries
                .get_mut(&intent_id)
                .map_or(DeliveryCompletionOutcome::Fenced, |state| {
                    state.succeed(token, provider_message_id, succeeded_at)
                }))
        })
    }

    fn fail_retryable(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        attempt: u32,
        error: BoundedDeliveryText,
        next_retry_at: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<DeliveryCompletionOutcome, PortError<Self::Error>>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("state lock")
                .deliveries
                .get_mut(&intent_id)
                .map_or(DeliveryCompletionOutcome::Fenced, |state| {
                    state.fail_retryable(token, attempt, error, next_retry_at)
                }))
        })
    }

    fn fail_terminal(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        attempt: u32,
        error: BoundedDeliveryText,
        failed_at: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<DeliveryCompletionOutcome, PortError<Self::Error>>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("state lock")
                .deliveries
                .get_mut(&intent_id)
                .map_or(DeliveryCompletionOutcome::Fenced, |state| {
                    state.fail_terminal(token, attempt, error, failed_at)
                }))
        })
    }

    fn skip(
        &self,
        intent_id: OutboxIntentId,
        token: DeliveryLeaseToken,
        reason: BoundedDeliveryText,
        skipped_at: DateTime<Utc>,
    ) -> BoxFuture<'_, Result<DeliveryCompletionOutcome, PortError<Self::Error>>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .expect("state lock")
                .deliveries
                .get_mut(&intent_id)
                .map_or(DeliveryCompletionOutcome::Fenced, |state| {
                    state.skip(token, reason, skipped_at)
                }))
        })
    }
}

impl DeliveryCheckpointPort for MemoryAdapter {
    type Error = Infallible;

    fn checkpoint(
        &self,
        query: CheckpointQuery,
    ) -> BoxFuture<'_, Result<Option<DeliveryCheckpoint>, PortError<Self::Error>>> {
        Box::pin(async move {
            let state = self.state.lock().expect("state lock");
            Ok(state
                .checkpoints
                .get(query.worker().as_str())
                .copied()
                .map(|cursor| DeliveryCheckpoint::new(query.worker().clone(), cursor)))
        })
    }

    fn advance_checkpoint(
        &self,
        advance: CheckpointAdvance,
    ) -> BoxFuture<'_, Result<CheckpointAdvanceOutcome, PortError<Self::Error>>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("state lock");
            let terminal = state
                .deliveries
                .get(&advance.terminal_intent_id())
                .is_some_and(|delivery| delivery.status().is_terminal());
            if !terminal {
                return Ok(CheckpointAdvanceOutcome::TerminalStateRequired);
            }
            let current = state.checkpoints.get(advance.worker().as_str()).copied();
            if current == Some(advance.next()) {
                return Ok(CheckpointAdvanceOutcome::Repeated);
            }
            if current != advance.expected() {
                return Ok(CheckpointAdvanceOutcome::Conflict);
            }
            state
                .checkpoints
                .insert(advance.worker().as_str().to_string(), advance.next());
            Ok(CheckpointAdvanceOutcome::Advanced)
        })
    }
}
