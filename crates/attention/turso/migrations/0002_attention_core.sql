CREATE TABLE attention_stream_state (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    head_cursor BLOB NOT NULL CHECK(length(head_cursor) = 8),
    floor_cursor BLOB NOT NULL CHECK(length(floor_cursor) = 8),
    CHECK(floor_cursor <= head_cursor)
);

INSERT INTO attention_stream_state (singleton, head_cursor, floor_cursor)
VALUES (1, x'0000000000000001', x'0000000000000001');

CREATE TABLE mutation_outcomes (
    mutation_key TEXT PRIMARY KEY NOT NULL,
    operation INTEGER NOT NULL CHECK(operation BETWEEN 0 AND 8),
    fingerprint BLOB NOT NULL CHECK(length(fingerprint) = 32),
    outcome_version INTEGER NOT NULL CHECK(outcome_version > 0),
    outcome_bytes BLOB NOT NULL
);

CREATE TABLE source_receipts (
    id TEXT PRIMARY KEY NOT NULL,
    source_kind TEXT NOT NULL,
    source_instance TEXT NOT NULL,
    occurrence_id TEXT NOT NULL,
    entity_source_kind TEXT,
    entity_source_instance TEXT,
    external_entity_id TEXT,
    fingerprint BLOB NOT NULL CHECK(length(fingerprint) = 32),
    order_mode INTEGER NOT NULL CHECK(order_mode IN (0, 1)),
    order_domain TEXT,
    order_value BLOB,
    occurred_at TEXT NOT NULL,
    ingested_at TEXT NOT NULL,
    accepted_mutation_key TEXT NOT NULL,
    UNIQUE(source_kind, source_instance, occurrence_id),
    CHECK(
        (entity_source_kind IS NULL AND entity_source_instance IS NULL AND external_entity_id IS NULL)
        OR
        (entity_source_kind IS NOT NULL AND entity_source_instance IS NOT NULL AND external_entity_id IS NOT NULL)
    ),
    CHECK(
        (order_mode = 0 AND order_domain IS NULL AND order_value IS NULL)
        OR
        (order_mode = 1 AND order_domain IS NOT NULL)
    ),
    CHECK(order_value IS NULL OR length(order_value) > 0),
    FOREIGN KEY(accepted_mutation_key) REFERENCES mutation_outcomes(mutation_key)
);

CREATE TABLE source_entities (
    id TEXT PRIMARY KEY NOT NULL,
    source_kind TEXT NOT NULL,
    source_instance TEXT NOT NULL,
    external_entity_id TEXT NOT NULL,
    state_version BLOB NOT NULL CHECK(length(state_version) = 8),
    latest_receipt_id TEXT NOT NULL,
    order_mode INTEGER NOT NULL CHECK(order_mode IN (0, 1)),
    order_domain TEXT,
    order_value BLOB,
    UNIQUE(source_kind, source_instance, external_entity_id),
    CHECK(
        (order_mode = 0 AND order_domain IS NULL AND order_value IS NULL)
        OR
        (order_mode = 1 AND order_domain IS NOT NULL)
    ),
    CHECK(order_value IS NULL OR length(order_value) > 0),
    FOREIGN KEY(latest_receipt_id) REFERENCES source_receipts(id)
);

CREATE TABLE work_items (
    id TEXT PRIMARY KEY NOT NULL,
    revision BLOB NOT NULL CHECK(length(revision) = 8),
    lifecycle INTEGER NOT NULL CHECK(lifecycle IN (0, 1, 2)),
    due_at TEXT,
    scheduled_at TEXT,
    defer_until TEXT,
    source_kind TEXT,
    source_instance TEXT,
    external_entity_id TEXT,
    CHECK(
        (source_kind IS NULL AND source_instance IS NULL AND external_entity_id IS NULL)
        OR
        (source_kind IS NOT NULL AND source_instance IS NOT NULL AND external_entity_id IS NOT NULL)
    ),
    FOREIGN KEY(source_kind, source_instance, external_entity_id)
        REFERENCES source_entities(source_kind, source_instance, external_entity_id)
);

CREATE INDEX work_items_by_lifecycle ON work_items(lifecycle, id);

CREATE TABLE attention_signals (
    id TEXT PRIMARY KEY NOT NULL,
    revision BLOB NOT NULL CHECK(length(revision) = 8),
    source_lifecycle INTEGER NOT NULL CHECK(source_lifecycle IN (0, 1, 2)),
    attention_state INTEGER NOT NULL CHECK(attention_state IN (0, 1)),
    source_receipt_id TEXT NOT NULL,
    source_entity_id TEXT,
    FOREIGN KEY(source_receipt_id) REFERENCES source_receipts(id),
    FOREIGN KEY(source_entity_id) REFERENCES source_entities(id)
);

CREATE INDEX attention_signals_by_attention_state
    ON attention_signals(attention_state, id);

CREATE TABLE reminders (
    id TEXT PRIMARY KEY NOT NULL,
    revision BLOB NOT NULL CHECK(length(revision) = 8),
    target_kind INTEGER NOT NULL CHECK(target_kind IN (0, 1)),
    target_id TEXT NOT NULL,
    trigger_at TEXT NOT NULL,
    current_fire_id TEXT,
    UNIQUE(target_kind, target_id),
    FOREIGN KEY(current_fire_id) REFERENCES reminder_fires(id)
);

CREATE TABLE reminder_fires (
    id TEXT PRIMARY KEY NOT NULL,
    reminder_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    trigger_at TEXT NOT NULL,
    state INTEGER NOT NULL CHECK(state IN (0, 1, 2, 3)),
    UNIQUE(reminder_id, ordinal),
    FOREIGN KEY(reminder_id) REFERENCES reminders(id)
);

CREATE INDEX reminder_fires_by_reminder_state
    ON reminder_fires(reminder_id, state, id);
CREATE INDEX reminder_fires_by_state_trigger
    ON reminder_fires(state, trigger_at, id, reminder_id);

CREATE TABLE change_events (
    cursor BLOB PRIMARY KEY NOT NULL CHECK(length(cursor) = 8),
    event_id TEXT NOT NULL UNIQUE,
    occurred_at TEXT NOT NULL,
    kind INTEGER NOT NULL CHECK(kind BETWEEN 0 AND 8),
    payload_version INTEGER NOT NULL CHECK(payload_version > 0),
    payload_bytes BLOB NOT NULL
);

CREATE TABLE outbox_intents (
    id TEXT PRIMARY KEY NOT NULL,
    deduplication_key TEXT NOT NULL UNIQUE,
    subject_kind INTEGER NOT NULL CHECK(subject_kind IN (0, 1)),
    subject_id TEXT NOT NULL,
    originating_event_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    purpose INTEGER NOT NULL CHECK(purpose IN (0, 1)),
    FOREIGN KEY(originating_event_id) REFERENCES change_events(event_id)
);
