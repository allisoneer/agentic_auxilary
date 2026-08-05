# Repair duplicate current reminder fires before migration 0003

Use this procedure only when migration 0003 cannot create
`reminder_fires_one_current_per_reminder` because a head-two database contains
more than one `reminder_fires` row in state `0` or `1` for a reminder. Migration
0003 and its ledger insertion share one immediate transaction, so this failure
leaves the database at head two and the migration retryable.

This repository ships no repair CLI, SQL shell, maintenance recipe, example
binary, or public raw-SQL API. The procedure requires an operator-supplied Rust
maintenance client using exactly `turso-db` `0.8.0-pre.1` with the local
`Builder::new_local` API. Generic SQLite tools and other Turso versions are not
supported and must not be substituted.

## Preconditions

Do not begin unless every precondition is satisfied:

1. Stop the process that owns the database and guarantee that no other process
   or client can access the database throughout the repair. The adapter's
   directory ownership lock is private and is not acquired by the external
   maintenance client.
2. Using the same `Config`, open the database with `AttentionDatabase::open`
   without invoking startup migrations, close it with `AttentionDatabase::close`,
   and create a uniquely named backup with `AttentionDatabase::backup`. Verify
   that the backup completed successfully and retain it until the repaired
   database has passed migration and semantic-read checks.
3. Ensure the maintenance client embeds or reads these repository artifacts
   without modifying them:
   - [`repair_duplicate_current_reminder_fires_setup.sql`](repair_duplicate_current_reminder_fires_setup.sql)
   - [`repair_duplicate_current_reminder_fires_apply.sql`](repair_duplicate_current_reminder_fires_apply.sql)
4. Establish domain evidence sufficient to choose one authoritative current
   fire and a truthful terminal state for every other current fire. If any
   prerequisite or decision is unavailable, stop and restore or escalate.

## Transaction procedure

The maintenance client owns the transaction. Neither SQL artifact begins,
commits, or rolls back one.

1. Connect to the stopped database through exact-pinned local
   `turso_db::Builder::new_local` and begin one
   `TransactionBehavior::Immediate` transaction.
2. Execute the complete setup artifact. It creates only TEMP inspection,
   snapshot, and decision objects; it does not modify persistent data.
3. Inspect `temp.__attention_repair_duplicate_reminders` and the complete
   `temp.__attention_repair_affected_fire_history`, reading the history in
   `reminder_id`, `ordinal`, then `fire_id` order. Review every row for every
   affected reminder. Ordinal, timestamp, state, and
   `reminders.current_fire_id` are evidence only and are never automatic winner
   rules.
4. For each affected reminder, choose exactly one authoritative fire that is
   currently in state `0` (Scheduled) or `1` (Fired). For every other current
   fire, choose the evidence-backed terminal state `2` (Acknowledged) or `3`
   (Snoozed). These terminal states have different domain meanings; never guess
   or select one merely to make migration succeed.
5. Use prepared statements and bound parameters to populate
   `temp.__attention_repair_authoritative` with each reminder ID and its
   authoritative fire ID, and `temp.__attention_repair_retire` with each
   reminder ID, retired fire ID, and chosen terminal state. Never interpolate
   an identifier or stored value into SQL text.
6. Execute the complete apply artifact. It validates the live rows against the
   setup snapshots, requires exact decision sets, updates only selected fire
   states and reminder pointers, and checks history and consistency
   postconditions. Its assertion failures are static and do not include stored
   IDs.
7. Commit only if setup, all bound decision inserts, apply, and all operator
   checks succeed. On any error or uncertainty, explicitly roll back the
   transaction, stop, and restore the verified backup or escalate. Do not edit
   decisions and continue within a transaction after a failed apply.

## After a successful commit

Close the maintenance client before returning ownership to the application.
Open the database through `AttentionDatabase`, invoke
`run_startup_migrations`, and verify that migration head 3 is reached. A second
migration invocation must apply zero migrations. Perform a semantic reminder
read for every repaired reminder and confirm the selected current fire and all
retained fire history reconstruct successfully before removing the backup or
resuming normal processing.

If migration or semantic verification fails, stop the process again, preserve
the failed database for investigation, restore the verified pre-repair backup,
and escalate. Do not attempt an unqualified direct edit.
