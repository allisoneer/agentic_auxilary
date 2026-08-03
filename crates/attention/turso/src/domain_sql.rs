pub const SELECT_STREAM_STATE: &str =
    "SELECT head_cursor, floor_cursor FROM attention_stream_state WHERE singleton = 1";
pub const UPDATE_STREAM_HEAD: &str =
    "UPDATE attention_stream_state SET head_cursor = ?1 WHERE singleton = 1 AND head_cursor = ?2";

pub const SELECT_WORK_ITEM: &str = "SELECT id, revision, lifecycle, due_at, scheduled_at, \
    defer_until, source_kind, source_instance, external_entity_id FROM work_items WHERE id = ?1";
pub const SELECT_WORK_ITEMS: &str = "SELECT id, revision, lifecycle, due_at, scheduled_at, \
    defer_until, source_kind, source_instance, external_entity_id FROM work_items ORDER BY id";
pub const INSERT_WORK_ITEM: &str = "INSERT INTO work_items (id, revision, lifecycle, due_at, \
    scheduled_at, defer_until, source_kind, source_instance, external_entity_id) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT(id) DO NOTHING";
pub const UPDATE_WORK_ITEM: &str = "UPDATE work_items SET revision = ?2, lifecycle = ?3, \
    due_at = ?4, scheduled_at = ?5, defer_until = ?6, source_kind = ?7, source_instance = ?8, \
    external_entity_id = ?9 WHERE id = ?1 AND revision = ?10";

pub const SELECT_SIGNAL: &str = "SELECT id, revision, source_lifecycle, attention_state, \
    source_receipt_id, source_entity_id FROM attention_signals WHERE id = ?1";
pub const SELECT_SIGNALS: &str = "SELECT id, revision, source_lifecycle, attention_state, \
    source_receipt_id, source_entity_id FROM attention_signals ORDER BY id";
pub const INSERT_SIGNAL: &str = "INSERT INTO attention_signals (id, revision, source_lifecycle, \
    attention_state, source_receipt_id, source_entity_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
    ON CONFLICT(id) DO UPDATE SET revision = excluded.revision, \
    source_lifecycle = excluded.source_lifecycle, attention_state = excluded.attention_state, \
    source_receipt_id = excluded.source_receipt_id, source_entity_id = excluded.source_entity_id";
pub const UPDATE_SIGNAL_GUARDED: &str = "UPDATE attention_signals SET revision = ?2, \
    source_lifecycle = ?3, attention_state = ?4, source_receipt_id = ?5, source_entity_id = ?6 \
    WHERE id = ?1 AND revision = ?7";

pub const SELECT_RECEIPT: &str = "SELECT id, source_kind, source_instance, occurrence_id, \
    entity_source_kind, entity_source_instance, external_entity_id, fingerprint, order_mode, \
    order_domain, order_value, occurred_at, ingested_at FROM source_receipts WHERE id = ?1";
pub const SELECT_RECEIPT_BY_OCCURRENCE: &str = "SELECT id, source_kind, source_instance, \
    occurrence_id, entity_source_kind, entity_source_instance, external_entity_id, fingerprint, \
    order_mode, order_domain, order_value, occurred_at, ingested_at, accepted_mutation_key \
    FROM source_receipts WHERE source_kind = ?1 AND source_instance = ?2 AND occurrence_id = ?3";
pub const INSERT_RECEIPT: &str = "INSERT INTO source_receipts (id, source_kind, source_instance, \
    occurrence_id, entity_source_kind, entity_source_instance, external_entity_id, fingerprint, \
    order_mode, order_domain, order_value, occurred_at, ingested_at, accepted_mutation_key) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) \
    ON CONFLICT(source_kind, source_instance, occurrence_id) DO NOTHING";

pub const SELECT_ENTITY: &str = "SELECT id, source_kind, source_instance, external_entity_id, \
    state_version, latest_receipt_id, order_mode, order_domain, order_value FROM source_entities \
    WHERE source_kind = ?1 AND source_instance = ?2 AND external_entity_id = ?3";
pub const INSERT_ENTITY: &str = "INSERT INTO source_entities (id, source_kind, source_instance, \
    external_entity_id, state_version, latest_receipt_id, order_mode, order_domain, order_value) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT(source_kind, source_instance, \
    external_entity_id) DO UPDATE SET state_version = excluded.state_version, \
    latest_receipt_id = excluded.latest_receipt_id, order_mode = excluded.order_mode, \
    order_domain = excluded.order_domain, order_value = excluded.order_value";

pub const SELECT_REMINDER: &str = "SELECT id, revision, target_kind, target_id, trigger_at, \
    current_fire_id FROM reminders WHERE id = ?1";
pub const SELECT_REMINDERS: &str = "SELECT id, revision, target_kind, target_id, trigger_at, \
    current_fire_id FROM reminders ORDER BY id";
pub const SELECT_REMINDER_FIRES: &str = "SELECT id, trigger_at, state FROM reminder_fires \
    WHERE reminder_id = ?1 ORDER BY ordinal";
pub const INSERT_REMINDER: &str = "INSERT INTO reminders (id, revision, target_kind, target_id, \
    trigger_at, current_fire_id) VALUES (?1, ?2, ?3, ?4, ?5, NULL) ON CONFLICT DO NOTHING";
pub const UPDATE_REMINDER: &str = "UPDATE reminders SET revision = ?2, target_kind = ?3, \
    target_id = ?4, trigger_at = ?5, current_fire_id = ?6 WHERE id = ?1 AND revision = ?7 \
    AND current_fire_id = ?8";
pub const SET_CURRENT_FIRE: &str =
    "UPDATE reminders SET current_fire_id = ?2 WHERE id = ?1 AND current_fire_id IS NULL";
pub const UPSERT_FIRE: &str = "INSERT INTO reminder_fires (id, reminder_id, ordinal, trigger_at, \
    state) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO UPDATE SET \
    ordinal = excluded.ordinal, trigger_at = excluded.trigger_at, state = excluded.state \
    WHERE reminder_fires.reminder_id = excluded.reminder_id";

pub const INSERT_OUTCOME: &str = "INSERT INTO mutation_outcomes (mutation_key, operation, \
    fingerprint, outcome_version, outcome_bytes) VALUES (?1, ?2, ?3, ?4, ?5) \
    ON CONFLICT(mutation_key) DO NOTHING";
pub const SELECT_OUTCOME: &str = "SELECT operation, fingerprint, outcome_version, outcome_bytes \
    FROM mutation_outcomes WHERE mutation_key = ?1";
pub const INSERT_EVENT: &str = "INSERT INTO change_events (cursor, event_id, occurred_at, kind, \
    payload_version, payload_bytes) VALUES (?1, ?2, ?3, ?4, ?5, ?6)";
pub const SELECT_CHANGES: &str = "SELECT cursor, event_id, occurred_at, kind, payload_version, \
    payload_bytes FROM change_events WHERE cursor > ?1 AND cursor <= ?2 ORDER BY cursor LIMIT ?3";
pub const INSERT_OUTBOX: &str = "INSERT INTO outbox_intents (id, deduplication_key, subject_kind, \
    subject_id, originating_event_id, created_at, purpose) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";
