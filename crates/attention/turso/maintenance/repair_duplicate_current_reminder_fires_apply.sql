DROP TABLE IF EXISTS temp.__attention_repair_assertions;

CREATE TEMP TABLE __attention_repair_assertions (
    affected_set_drift INTEGER NOT NULL,
    reminder_snapshot_drift INTEGER NOT NULL,
    fire_snapshot_drift INTEGER NOT NULL,
    authoritative_set_invalid INTEGER NOT NULL,
    authoritative_fire_invalid INTEGER NOT NULL,
    retirement_fire_invalid INTEGER NOT NULL,
    authoritative_fire_retired INTEGER NOT NULL,
    retirement_set_invalid INTEGER NOT NULL,
    terminal_state_invalid INTEGER NOT NULL,
    CONSTRAINT repair_affected_set_unchanged CHECK(affected_set_drift = 0),
    CONSTRAINT repair_reminder_snapshot_unchanged CHECK(reminder_snapshot_drift = 0),
    CONSTRAINT repair_fire_snapshot_unchanged CHECK(fire_snapshot_drift = 0),
    CONSTRAINT repair_authoritative_set_exact CHECK(authoritative_set_invalid = 0),
    CONSTRAINT repair_authoritative_fires_current CHECK(authoritative_fire_invalid = 0),
    CONSTRAINT repair_retirement_fires_current CHECK(retirement_fire_invalid = 0),
    CONSTRAINT repair_authoritative_not_retired CHECK(authoritative_fire_retired = 0),
    CONSTRAINT repair_retirement_set_exact CHECK(retirement_set_invalid = 0),
    CONSTRAINT repair_terminal_states_valid CHECK(terminal_state_invalid = 0)
);

INSERT INTO temp.__attention_repair_assertions
SELECT
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_baseline_reminders AS b
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminders AS r
            WHERE r.id = b.reminder_id
              AND 1 < (
                  SELECT count(*)
                  FROM reminder_fires AS f
                  WHERE f.reminder_id = r.id
                    AND f.state IN (0, 1)
              )
        )
    ) OR EXISTS (
        SELECT 1
        FROM reminders AS r
        WHERE 1 < (
            SELECT count(*)
            FROM reminder_fires AS f
            WHERE f.reminder_id = r.id
              AND f.state IN (0, 1)
        )
          AND NOT EXISTS (
              SELECT 1
              FROM temp.__attention_repair_baseline_reminders AS b
              WHERE b.reminder_id = r.id
          )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_baseline_reminders AS b
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminders AS r
            WHERE r.id = b.reminder_id
              AND r.revision = b.revision
              AND r.current_fire_id IS b.current_fire_id
        )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_baseline_fires AS b
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminder_fires AS f
            WHERE f.id = b.fire_id
              AND f.reminder_id = b.reminder_id
              AND f.ordinal = b.ordinal
              AND f.trigger_at = b.trigger_at
              AND f.state = b.state
        )
    ) OR EXISTS (
        SELECT 1
        FROM reminder_fires AS f
        JOIN temp.__attention_repair_baseline_reminders AS r
          ON r.reminder_id = f.reminder_id
        WHERE NOT EXISTS (
            SELECT 1
            FROM temp.__attention_repair_baseline_fires AS b
            WHERE b.fire_id = f.id
              AND b.reminder_id = f.reminder_id
              AND b.ordinal = f.ordinal
              AND b.trigger_at = f.trigger_at
              AND b.state = f.state
        )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_baseline_reminders AS b
        WHERE NOT EXISTS (
            SELECT 1
            FROM temp.__attention_repair_authoritative AS a
            WHERE a.reminder_id = b.reminder_id
        )
    ) OR EXISTS (
        SELECT 1
        FROM temp.__attention_repair_authoritative AS a
        WHERE NOT EXISTS (
            SELECT 1
            FROM temp.__attention_repair_baseline_reminders AS b
            WHERE b.reminder_id = a.reminder_id
        )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_authoritative AS a
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminder_fires AS f
            WHERE f.id = a.authoritative_fire_id
              AND f.reminder_id = a.reminder_id
              AND f.state IN (0, 1)
        )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_retire AS d
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminder_fires AS f
            WHERE f.id = d.retired_fire_id
              AND f.reminder_id = d.reminder_id
              AND f.state IN (0, 1)
        )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_authoritative AS a
        JOIN temp.__attention_repair_retire AS d
          ON d.reminder_id = a.reminder_id
         AND d.retired_fire_id = a.authoritative_fire_id
    ),
    EXISTS (
        SELECT 1
        FROM reminder_fires AS f
        JOIN temp.__attention_repair_authoritative AS a
          ON a.reminder_id = f.reminder_id
        WHERE f.state IN (0, 1)
          AND f.id != a.authoritative_fire_id
          AND NOT EXISTS (
              SELECT 1
              FROM temp.__attention_repair_retire AS d
              WHERE d.reminder_id = f.reminder_id
                AND d.retired_fire_id = f.id
          )
    ) OR EXISTS (
        SELECT 1
        FROM temp.__attention_repair_retire AS d
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminder_fires AS f
            JOIN temp.__attention_repair_authoritative AS a
              ON a.reminder_id = f.reminder_id
            WHERE f.reminder_id = d.reminder_id
              AND f.id = d.retired_fire_id
              AND f.state IN (0, 1)
              AND f.id != a.authoritative_fire_id
        )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_retire
        WHERE terminal_state IS NULL
           OR terminal_state NOT BETWEEN 2 AND 3
    );

UPDATE reminder_fires
SET state = (
    SELECT d.terminal_state
    FROM temp.__attention_repair_retire AS d
    WHERE d.retired_fire_id = reminder_fires.id
      AND d.reminder_id = reminder_fires.reminder_id
)
WHERE EXISTS (
    SELECT 1
    FROM temp.__attention_repair_retire AS d
    WHERE d.retired_fire_id = reminder_fires.id
      AND d.reminder_id = reminder_fires.reminder_id
);

UPDATE reminders
SET current_fire_id = (
    SELECT a.authoritative_fire_id
    FROM temp.__attention_repair_authoritative AS a
    WHERE a.reminder_id = reminders.id
)
WHERE EXISTS (
    SELECT 1
    FROM temp.__attention_repair_authoritative AS a
    WHERE a.reminder_id = reminders.id
);

CREATE TEMP TABLE __attention_repair_postconditions (
    current_fire_invalid INTEGER NOT NULL,
    retirement_state_invalid INTEGER NOT NULL,
    row_identity_changed INTEGER NOT NULL,
    fire_shape_changed INTEGER NOT NULL,
    reminder_revision_changed INTEGER NOT NULL,
    unselected_fire_state_changed INTEGER NOT NULL,
    CONSTRAINT repair_one_current_fire_matching_pointer CHECK(current_fire_invalid = 0),
    CONSTRAINT repair_retirement_states_applied CHECK(retirement_state_invalid = 0),
    CONSTRAINT repair_row_identity_preserved CHECK(row_identity_changed = 0),
    CONSTRAINT repair_fire_history_shape_preserved CHECK(fire_shape_changed = 0),
    CONSTRAINT repair_reminder_revisions_preserved CHECK(reminder_revision_changed = 0),
    CONSTRAINT repair_unselected_fire_states_preserved CHECK(unselected_fire_state_changed = 0)
);

INSERT INTO temp.__attention_repair_postconditions
SELECT
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_authoritative AS a
        JOIN reminders AS r
          ON r.id = a.reminder_id
        WHERE r.current_fire_id IS NOT a.authoritative_fire_id
           OR 1 != (
               SELECT count(*)
               FROM reminder_fires AS f
               WHERE f.reminder_id = a.reminder_id
                 AND f.state IN (0, 1)
           )
           OR NOT EXISTS (
               SELECT 1
               FROM reminder_fires AS f
               WHERE f.id = a.authoritative_fire_id
                 AND f.reminder_id = a.reminder_id
                 AND f.state IN (0, 1)
           )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_retire AS d
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminder_fires AS f
            WHERE f.id = d.retired_fire_id
              AND f.reminder_id = d.reminder_id
              AND f.state = d.terminal_state
        )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_baseline_reminder_ids AS b
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminders AS r
            WHERE r.id = b.reminder_id
        )
    ) OR EXISTS (
        SELECT 1
        FROM reminders AS r
        WHERE NOT EXISTS (
            SELECT 1
            FROM temp.__attention_repair_baseline_reminder_ids AS b
            WHERE b.reminder_id = r.id
        )
    ) OR EXISTS (
        SELECT 1
        FROM temp.__attention_repair_baseline_fire_ids AS b
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminder_fires AS f
            WHERE f.id = b.fire_id
        )
    ) OR EXISTS (
        SELECT 1
        FROM reminder_fires AS f
        WHERE NOT EXISTS (
            SELECT 1
            FROM temp.__attention_repair_baseline_fire_ids AS b
            WHERE b.fire_id = f.id
        )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_baseline_fires AS b
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminder_fires AS f
            WHERE f.id = b.fire_id
              AND f.reminder_id = b.reminder_id
              AND f.ordinal = b.ordinal
              AND f.trigger_at = b.trigger_at
        )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_baseline_reminders AS b
        WHERE NOT EXISTS (
            SELECT 1
            FROM reminders AS r
            WHERE r.id = b.reminder_id
              AND r.revision = b.revision
        )
    ),
    EXISTS (
        SELECT 1
        FROM temp.__attention_repair_baseline_fire_ids AS b
        WHERE NOT EXISTS (
            SELECT 1
            FROM temp.__attention_repair_retire AS d
            WHERE d.retired_fire_id = b.fire_id
        )
          AND NOT EXISTS (
              SELECT 1
              FROM reminder_fires AS f
              WHERE f.id = b.fire_id
                AND f.state = b.state
          )
    );
