CREATE TABLE delivery_states (
    intent_id TEXT PRIMARY KEY NOT NULL,
    status INTEGER NOT NULL CHECK(status IN (0, 1, 2, 3, 4, 5)),
    lease_token BLOB CHECK(lease_token IS NULL OR length(lease_token) = 32),
    lease_expires_at TEXT,
    attempt INTEGER CHECK(attempt IS NULL OR attempt BETWEEN 0 AND 4294967295),
    error TEXT CHECK(error IS NULL OR length(CAST(error AS BLOB)) <= 65536),
    next_retry_at TEXT,
    provider_message_id TEXT CHECK(
        provider_message_id IS NULL OR length(CAST(provider_message_id AS BLOB)) <= 65536
    ),
    succeeded_at TEXT,
    reason TEXT CHECK(reason IS NULL OR length(CAST(reason AS BLOB)) <= 65536),
    skipped_at TEXT,
    failed_at TEXT,
    CHECK(status != 0 OR (
        lease_token IS NULL AND lease_expires_at IS NULL
        AND attempt IS NULL AND error IS NULL AND next_retry_at IS NULL
        AND provider_message_id IS NULL AND succeeded_at IS NULL
        AND reason IS NULL AND skipped_at IS NULL AND failed_at IS NULL
    )),
    CHECK(status != 1 OR (
        lease_token IS NOT NULL AND lease_expires_at IS NOT NULL
        AND attempt IS NULL AND error IS NULL AND next_retry_at IS NULL
        AND provider_message_id IS NULL AND succeeded_at IS NULL
        AND reason IS NULL AND skipped_at IS NULL AND failed_at IS NULL
    )),
    CHECK(status != 2 OR (
        lease_token IS NULL AND lease_expires_at IS NULL
        AND attempt IS NOT NULL AND error IS NOT NULL AND next_retry_at IS NOT NULL
        AND provider_message_id IS NULL AND succeeded_at IS NULL
        AND reason IS NULL AND skipped_at IS NULL AND failed_at IS NULL
    )),
    CHECK(status != 3 OR (
        lease_token IS NULL AND lease_expires_at IS NULL
        AND attempt IS NULL AND error IS NULL AND next_retry_at IS NULL
        AND provider_message_id IS NOT NULL AND succeeded_at IS NOT NULL
        AND reason IS NULL AND skipped_at IS NULL AND failed_at IS NULL
    )),
    CHECK(status != 4 OR (
        lease_token IS NULL AND lease_expires_at IS NULL
        AND attempt IS NULL AND error IS NULL AND next_retry_at IS NULL
        AND provider_message_id IS NULL AND succeeded_at IS NULL
        AND reason IS NOT NULL AND skipped_at IS NOT NULL AND failed_at IS NULL
    )),
    CHECK(status != 5 OR (
        lease_token IS NULL AND lease_expires_at IS NULL
        AND attempt IS NOT NULL AND error IS NOT NULL AND next_retry_at IS NULL
        AND provider_message_id IS NULL AND succeeded_at IS NULL
        AND reason IS NULL AND skipped_at IS NULL AND failed_at IS NOT NULL
    )),
    FOREIGN KEY(intent_id) REFERENCES outbox_intents(id)
);

CREATE TABLE delivery_checkpoints (
    worker TEXT PRIMARY KEY NOT NULL
        CHECK(length(CAST(worker AS BLOB)) <= 65536),
    cursor BLOB NOT NULL CHECK(length(cursor) = 8)
);

INSERT INTO delivery_states (intent_id, status)
SELECT id, 0 FROM outbox_intents;

CREATE INDEX delivery_states_by_status_intent
    ON delivery_states(status, intent_id);
CREATE INDEX delivery_states_by_lease_expiry
    ON delivery_states(status, lease_expires_at, intent_id) WHERE status = 1;
CREATE INDEX delivery_states_by_retry_due
    ON delivery_states(status, next_retry_at, intent_id) WHERE status = 2;
CREATE INDEX outbox_intents_by_created_at
    ON outbox_intents(created_at, id);
CREATE UNIQUE INDEX reminder_fires_one_current_per_reminder
    ON reminder_fires(reminder_id) WHERE state IN (0, 1);
