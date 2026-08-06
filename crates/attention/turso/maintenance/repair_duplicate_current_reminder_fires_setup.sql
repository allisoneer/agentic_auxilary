DROP TABLE IF EXISTS temp.__attention_repair_duplicate_reminders;
DROP TABLE IF EXISTS temp.__attention_repair_affected_fire_history;
DROP TABLE IF EXISTS temp.__attention_repair_postconditions;
DROP TABLE IF EXISTS temp.__attention_repair_assertions;
DROP TABLE IF EXISTS temp.__attention_repair_authoritative;
DROP TABLE IF EXISTS temp.__attention_repair_retire;
DROP TABLE IF EXISTS temp.__attention_repair_baseline_fires;
DROP TABLE IF EXISTS temp.__attention_repair_baseline_reminders;
DROP TABLE IF EXISTS temp.__attention_repair_baseline_fire_ids;
DROP TABLE IF EXISTS temp.__attention_repair_baseline_reminder_ids;

CREATE TEMP TABLE __attention_repair_baseline_reminder_ids (
    reminder_id TEXT PRIMARY KEY NOT NULL
);

INSERT INTO temp.__attention_repair_baseline_reminder_ids (reminder_id)
SELECT id
FROM reminders;

CREATE TEMP TABLE __attention_repair_baseline_fire_ids (
    fire_id TEXT PRIMARY KEY NOT NULL,
    state INTEGER NOT NULL
);

INSERT INTO temp.__attention_repair_baseline_fire_ids (fire_id, state)
SELECT id, state
FROM reminder_fires;

CREATE TEMP TABLE __attention_repair_baseline_reminders (
    reminder_id TEXT PRIMARY KEY NOT NULL,
    revision BLOB NOT NULL,
    current_fire_id TEXT
);

INSERT INTO temp.__attention_repair_baseline_reminders (
    reminder_id,
    revision,
    current_fire_id
)
SELECT r.id, r.revision, r.current_fire_id
FROM reminders AS r
WHERE 1 < (
    SELECT count(*)
    FROM reminder_fires AS f
    WHERE f.reminder_id = r.id
      AND f.state IN (0, 1)
);

CREATE TEMP TABLE __attention_repair_baseline_fires (
    fire_id TEXT PRIMARY KEY NOT NULL,
    reminder_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    trigger_at TEXT NOT NULL,
    state INTEGER NOT NULL
);

INSERT INTO temp.__attention_repair_baseline_fires (
    fire_id,
    reminder_id,
    ordinal,
    trigger_at,
    state
)
SELECT f.id, f.reminder_id, f.ordinal, f.trigger_at, f.state
FROM reminder_fires AS f
JOIN temp.__attention_repair_baseline_reminders AS b
  ON b.reminder_id = f.reminder_id;

CREATE TEMP TABLE __attention_repair_duplicate_reminders (
    reminder_id TEXT PRIMARY KEY NOT NULL,
    revision BLOB NOT NULL,
    current_fire_id TEXT,
    scheduled_fire_count INTEGER NOT NULL,
    fired_fire_count INTEGER NOT NULL,
    current_fire_count INTEGER NOT NULL
);

INSERT INTO temp.__attention_repair_duplicate_reminders (
    reminder_id,
    revision,
    current_fire_id,
    scheduled_fire_count,
    fired_fire_count,
    current_fire_count
)
SELECT
    b.reminder_id,
    b.revision,
    b.current_fire_id,
    sum(CASE WHEN f.state = 0 THEN 1 ELSE 0 END) AS scheduled_fire_count,
    sum(CASE WHEN f.state = 1 THEN 1 ELSE 0 END) AS fired_fire_count,
    sum(CASE WHEN f.state IN (0, 1) THEN 1 ELSE 0 END) AS current_fire_count
FROM __attention_repair_baseline_reminders AS b
JOIN __attention_repair_baseline_fires AS f
  ON f.reminder_id = b.reminder_id
GROUP BY b.reminder_id, b.revision, b.current_fire_id
ORDER BY b.reminder_id;

CREATE TEMP TABLE __attention_repair_affected_fire_history (
    reminder_id TEXT NOT NULL,
    revision BLOB NOT NULL,
    current_fire_id TEXT,
    fire_id TEXT PRIMARY KEY NOT NULL,
    ordinal INTEGER NOT NULL,
    trigger_at TEXT NOT NULL,
    state INTEGER NOT NULL
);

INSERT INTO temp.__attention_repair_affected_fire_history (
    reminder_id,
    revision,
    current_fire_id,
    fire_id,
    ordinal,
    trigger_at,
    state
)
SELECT
    b.reminder_id,
    b.revision,
    b.current_fire_id,
    f.fire_id,
    f.ordinal,
    f.trigger_at,
    f.state
FROM __attention_repair_baseline_reminders AS b
JOIN __attention_repair_baseline_fires AS f
  ON f.reminder_id = b.reminder_id
ORDER BY b.reminder_id, f.ordinal, f.fire_id;

CREATE TEMP TABLE __attention_repair_authoritative (
    reminder_id TEXT PRIMARY KEY NOT NULL,
    authoritative_fire_id TEXT NOT NULL
);

CREATE TEMP TABLE __attention_repair_retire (
    reminder_id TEXT NOT NULL,
    retired_fire_id TEXT PRIMARY KEY NOT NULL,
    terminal_state INTEGER NOT NULL CHECK(terminal_state IN (2, 3))
);
