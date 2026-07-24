//! Native operation-specific mutation commands.

use crate::AttentionSignalId;
use crate::CanonicalFingerprint;
use crate::MutationIdempotencyKey;
use crate::MutationOperation;
use crate::OccurrenceKey;
use crate::ReminderFireId;
use crate::ReminderId;
use crate::ReminderTarget;
use crate::Revision;
use crate::SignalSourceLifecycle;
use crate::SourceEntityId;
use crate::SourceEntityKey;
use crate::SourceOrderMode;
use crate::SourceReceiptId;
use crate::WorkItemId;
use crate::idempotency::CanonicalWriter;
use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;

pub trait CanonicalCommand {
    fn operation(&self) -> MutationOperation;
    fn idempotency_key(&self) -> MutationIdempotencyKey;
    fn canonical_fingerprint(&self) -> CanonicalFingerprint;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateWorkItem {
    id: WorkItemId,
    due_at: Option<DateTime<Utc>>,
    scheduled_at: Option<DateTime<Utc>>,
    defer_until: Option<DateTime<Utc>>,
    source_link: Option<SourceEntityKey>,
    idempotency_key: MutationIdempotencyKey,
}

impl CreateWorkItem {
    pub const fn new(
        id: WorkItemId,
        due_at: Option<DateTime<Utc>>,
        scheduled_at: Option<DateTime<Utc>>,
        defer_until: Option<DateTime<Utc>>,
        source_link: Option<SourceEntityKey>,
        idempotency_key: MutationIdempotencyKey,
    ) -> Self {
        Self {
            id,
            due_at,
            scheduled_at,
            defer_until,
            source_link,
            idempotency_key,
        }
    }

    pub const fn id(&self) -> WorkItemId {
        self.id
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

    pub const fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }
}

impl CanonicalCommand for CreateWorkItem {
    fn operation(&self) -> MutationOperation {
        MutationOperation::CreateWorkItem
    }

    fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }

    fn canonical_fingerprint(&self) -> CanonicalFingerprint {
        let mut writer = CanonicalWriter::new(self.operation());
        write_uuid(&mut writer, self.id.as_uuid());
        write_optional_time(&mut writer, self.due_at.as_ref());
        write_optional_time(&mut writer, self.scheduled_at.as_ref());
        write_optional_time(&mut writer, self.defer_until.as_ref());
        write_optional_entity_key(&mut writer, self.source_link.as_ref());
        writer.finish()
    }
}

macro_rules! existing_root_command {
    ($name:ident, $id_type:ty, $operation:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            id: $id_type,
            expected_revision: Revision,
            idempotency_key: MutationIdempotencyKey,
        }

        impl $name {
            pub const fn new(
                id: $id_type,
                expected_revision: Revision,
                idempotency_key: MutationIdempotencyKey,
            ) -> Self {
                Self {
                    id,
                    expected_revision,
                    idempotency_key,
                }
            }

            pub const fn id(&self) -> $id_type {
                self.id
            }

            pub const fn expected_revision(&self) -> Revision {
                self.expected_revision
            }

            pub const fn idempotency_key(&self) -> MutationIdempotencyKey {
                self.idempotency_key
            }
        }

        impl CanonicalCommand for $name {
            fn operation(&self) -> MutationOperation {
                MutationOperation::$operation
            }

            fn idempotency_key(&self) -> MutationIdempotencyKey {
                self.idempotency_key
            }

            fn canonical_fingerprint(&self) -> CanonicalFingerprint {
                let mut writer = CanonicalWriter::new(self.operation());
                write_uuid(&mut writer, self.id.as_uuid());
                writer.bytes(&self.expected_revision.value().to_be_bytes());
                writer.finish()
            }
        }
    };
}

existing_root_command!(CompleteWorkItem, WorkItemId, CompleteWorkItem);
existing_root_command!(CancelWorkItem, WorkItemId, CancelWorkItem);
existing_root_command!(
    AcknowledgeAttentionSignal,
    AttentionSignalId,
    AcknowledgeAttentionSignal
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEntityIdentity {
    id: SourceEntityId,
    key: SourceEntityKey,
}

impl SourceEntityIdentity {
    pub const fn new(id: SourceEntityId, key: SourceEntityKey) -> Self {
        Self { id, key }
    }

    pub const fn id(&self) -> SourceEntityId {
        self.id
    }

    pub const fn key(&self) -> &SourceEntityKey {
        &self.key
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestSourceOccurrence {
    receipt_id: SourceReceiptId,
    entity: Option<SourceEntityIdentity>,
    signal_id: AttentionSignalId,
    occurrence_key: OccurrenceKey,
    occurred_at: DateTime<Utc>,
    ingested_at: DateTime<Utc>,
    order: SourceOrderMode,
    source_lifecycle: SignalSourceLifecycle,
    fresh_attention: bool,
    idempotency_key: MutationIdempotencyKey,
}

impl IngestSourceOccurrence {
    #[expect(
        clippy::too_many_arguments,
        reason = "complete source observation contract"
    )]
    pub const fn new(
        receipt_id: SourceReceiptId,
        entity: Option<SourceEntityIdentity>,
        signal_id: AttentionSignalId,
        occurrence_key: OccurrenceKey,
        occurred_at: DateTime<Utc>,
        ingested_at: DateTime<Utc>,
        order: SourceOrderMode,
        source_lifecycle: SignalSourceLifecycle,
        fresh_attention: bool,
        idempotency_key: MutationIdempotencyKey,
    ) -> Self {
        Self {
            receipt_id,
            entity,
            signal_id,
            occurrence_key,
            occurred_at,
            ingested_at,
            order,
            source_lifecycle,
            fresh_attention,
            idempotency_key,
        }
    }

    pub const fn receipt_id(&self) -> SourceReceiptId {
        self.receipt_id
    }

    pub const fn entity(&self) -> Option<&SourceEntityIdentity> {
        self.entity.as_ref()
    }

    pub const fn signal_id(&self) -> AttentionSignalId {
        self.signal_id
    }

    pub const fn occurrence_key(&self) -> &OccurrenceKey {
        &self.occurrence_key
    }

    pub const fn occurred_at(&self) -> &DateTime<Utc> {
        &self.occurred_at
    }

    pub const fn ingested_at(&self) -> &DateTime<Utc> {
        &self.ingested_at
    }

    pub const fn order(&self) -> &SourceOrderMode {
        &self.order
    }

    pub const fn source_lifecycle(&self) -> SignalSourceLifecycle {
        self.source_lifecycle
    }

    pub const fn fresh_attention(&self) -> bool {
        self.fresh_attention
    }

    pub const fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }
}

impl CanonicalCommand for IngestSourceOccurrence {
    fn operation(&self) -> MutationOperation {
        MutationOperation::IngestSourceOccurrence
    }

    fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }

    fn canonical_fingerprint(&self) -> CanonicalFingerprint {
        let mut writer = CanonicalWriter::new(self.operation());
        write_uuid(&mut writer, self.receipt_id.as_uuid());
        writer.bool(self.entity.is_some());
        if let Some(entity) = &self.entity {
            write_uuid(&mut writer, entity.id.as_uuid());
            write_entity_key(&mut writer, &entity.key);
        }
        write_uuid(&mut writer, self.signal_id.as_uuid());
        write_occurrence_key(&mut writer, &self.occurrence_key);
        write_time(&mut writer, &self.occurred_at);
        write_order(&mut writer, &self.order);
        writer.bytes(&[self.source_lifecycle as u8]);
        writer.bool(self.fresh_attention);
        writer.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateReminder {
    reminder_id: ReminderId,
    initial_fire_id: ReminderFireId,
    target: ReminderTarget,
    trigger_at: DateTime<Utc>,
    idempotency_key: MutationIdempotencyKey,
}

impl CreateReminder {
    pub const fn new(
        reminder_id: ReminderId,
        initial_fire_id: ReminderFireId,
        target: ReminderTarget,
        trigger_at: DateTime<Utc>,
        idempotency_key: MutationIdempotencyKey,
    ) -> Self {
        Self {
            reminder_id,
            initial_fire_id,
            target,
            trigger_at,
            idempotency_key,
        }
    }

    pub const fn reminder_id(&self) -> ReminderId {
        self.reminder_id
    }

    pub const fn initial_fire_id(&self) -> ReminderFireId {
        self.initial_fire_id
    }

    pub const fn target(&self) -> ReminderTarget {
        self.target
    }

    pub const fn trigger_at(&self) -> &DateTime<Utc> {
        &self.trigger_at
    }

    pub const fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }
}

impl CanonicalCommand for CreateReminder {
    fn operation(&self) -> MutationOperation {
        MutationOperation::CreateReminder
    }

    fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }

    fn canonical_fingerprint(&self) -> CanonicalFingerprint {
        let mut writer = CanonicalWriter::new(self.operation());
        write_uuid(&mut writer, self.reminder_id.as_uuid());
        write_uuid(&mut writer, self.initial_fire_id.as_uuid());
        write_target(&mut writer, self.target);
        write_time(&mut writer, &self.trigger_at);
        writer.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireReminder {
    reminder_id: ReminderId,
    fire_id: ReminderFireId,
    idempotency_key: MutationIdempotencyKey,
}

impl FireReminder {
    pub const fn new(
        reminder_id: ReminderId,
        fire_id: ReminderFireId,
        idempotency_key: MutationIdempotencyKey,
    ) -> Self {
        Self {
            reminder_id,
            fire_id,
            idempotency_key,
        }
    }

    pub const fn reminder_id(&self) -> ReminderId {
        self.reminder_id
    }

    pub const fn fire_id(&self) -> ReminderFireId {
        self.fire_id
    }

    pub const fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }
}

impl CanonicalCommand for FireReminder {
    fn operation(&self) -> MutationOperation {
        MutationOperation::FireReminder
    }

    fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }

    fn canonical_fingerprint(&self) -> CanonicalFingerprint {
        let mut writer = CanonicalWriter::new(self.operation());
        write_uuid(&mut writer, self.reminder_id.as_uuid());
        write_uuid(&mut writer, self.fire_id.as_uuid());
        writer.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcknowledgeReminderFire {
    reminder_id: ReminderId,
    fire_id: ReminderFireId,
    expected_revision: Revision,
    idempotency_key: MutationIdempotencyKey,
}

impl AcknowledgeReminderFire {
    pub const fn new(
        reminder_id: ReminderId,
        fire_id: ReminderFireId,
        expected_revision: Revision,
        idempotency_key: MutationIdempotencyKey,
    ) -> Self {
        Self {
            reminder_id,
            fire_id,
            expected_revision,
            idempotency_key,
        }
    }

    pub const fn reminder_id(&self) -> ReminderId {
        self.reminder_id
    }

    pub const fn fire_id(&self) -> ReminderFireId {
        self.fire_id
    }

    pub const fn expected_revision(&self) -> Revision {
        self.expected_revision
    }

    pub const fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }
}

impl CanonicalCommand for AcknowledgeReminderFire {
    fn operation(&self) -> MutationOperation {
        MutationOperation::AcknowledgeReminderFire
    }

    fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }

    fn canonical_fingerprint(&self) -> CanonicalFingerprint {
        let mut writer = CanonicalWriter::new(self.operation());
        write_uuid(&mut writer, self.reminder_id.as_uuid());
        write_uuid(&mut writer, self.fire_id.as_uuid());
        writer.bytes(&self.expected_revision.value().to_be_bytes());
        writer.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnoozeReminderFire {
    reminder_id: ReminderId,
    fire_id: ReminderFireId,
    replacement_fire_id: ReminderFireId,
    replacement_trigger_at: DateTime<Utc>,
    expected_revision: Revision,
    idempotency_key: MutationIdempotencyKey,
}

impl SnoozeReminderFire {
    pub const fn new(
        reminder_id: ReminderId,
        fire_id: ReminderFireId,
        replacement_fire_id: ReminderFireId,
        replacement_trigger_at: DateTime<Utc>,
        expected_revision: Revision,
        idempotency_key: MutationIdempotencyKey,
    ) -> Self {
        Self {
            reminder_id,
            fire_id,
            replacement_fire_id,
            replacement_trigger_at,
            expected_revision,
            idempotency_key,
        }
    }

    pub const fn reminder_id(&self) -> ReminderId {
        self.reminder_id
    }

    pub const fn fire_id(&self) -> ReminderFireId {
        self.fire_id
    }

    pub const fn replacement_fire_id(&self) -> ReminderFireId {
        self.replacement_fire_id
    }

    pub const fn replacement_trigger_at(&self) -> &DateTime<Utc> {
        &self.replacement_trigger_at
    }

    pub const fn expected_revision(&self) -> Revision {
        self.expected_revision
    }

    pub const fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }
}

impl CanonicalCommand for SnoozeReminderFire {
    fn operation(&self) -> MutationOperation {
        MutationOperation::SnoozeReminderFire
    }

    fn idempotency_key(&self) -> MutationIdempotencyKey {
        self.idempotency_key
    }

    fn canonical_fingerprint(&self) -> CanonicalFingerprint {
        let mut writer = CanonicalWriter::new(self.operation());
        write_uuid(&mut writer, self.reminder_id.as_uuid());
        write_uuid(&mut writer, self.fire_id.as_uuid());
        write_uuid(&mut writer, self.replacement_fire_id.as_uuid());
        write_time(&mut writer, &self.replacement_trigger_at);
        writer.bytes(&self.expected_revision.value().to_be_bytes());
        writer.finish()
    }
}

fn write_uuid(writer: &mut CanonicalWriter, value: uuid::Uuid) {
    writer.bytes(value.as_bytes());
}

fn write_time(writer: &mut CanonicalWriter, value: &DateTime<Utc>) {
    writer.bytes(value.to_rfc3339_opts(SecondsFormat::Nanos, true).as_bytes());
}

fn write_optional_time(writer: &mut CanonicalWriter, value: Option<&DateTime<Utc>>) {
    writer.bool(value.is_some());
    if let Some(value) = value {
        write_time(writer, value);
    }
}

fn write_entity_key(writer: &mut CanonicalWriter, key: &SourceEntityKey) {
    writer.bytes(key.source_kind().as_str().as_bytes());
    writer.bytes(key.source_instance().as_str().as_bytes());
    writer.bytes(key.external_entity_id().as_str().as_bytes());
}

fn write_optional_entity_key(writer: &mut CanonicalWriter, key: Option<&SourceEntityKey>) {
    writer.bool(key.is_some());
    if let Some(key) = key {
        write_entity_key(writer, key);
    }
}

fn write_occurrence_key(writer: &mut CanonicalWriter, key: &OccurrenceKey) {
    writer.bytes(key.source_kind().as_str().as_bytes());
    writer.bytes(key.source_instance().as_str().as_bytes());
    writer.bytes(key.occurrence_id().as_str().as_bytes());
}

fn write_order(writer: &mut CanonicalWriter, order: &SourceOrderMode) {
    match order {
        SourceOrderMode::Unordered => writer.bytes(&[0]),
        SourceOrderMode::Ordered { domain, value } => {
            writer.bytes(&[1]);
            writer.bytes(domain.as_str().as_bytes());
            writer.optional_bytes(value.as_ref().map(crate::NormalizedSourceOrder::as_bytes));
        }
    }
}

fn write_target(writer: &mut CanonicalWriter, target: ReminderTarget) {
    match target {
        ReminderTarget::WorkItem(id) => {
            writer.bytes(&[0]);
            write_uuid(writer, id.as_uuid());
        }
        ReminderTarget::AttentionSignal(id) => {
            writer.bytes(&[1]);
            write_uuid(writer, id.as_uuid());
        }
    }
}
