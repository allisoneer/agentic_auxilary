# attention-turso

`attention-turso` is Attention's native local Turso persistence adapter. It owns the database engine, exclusive directory ownership, migrations, transactional read/write ports, scheduler and delivery persistence, event cursors, stopped backup/restore, and storage failure classification.

## Qualified dependency

The workspace pins the pre-release engine exactly:

```toml
turso-db = { package = "turso", version = "=0.8.0-pre.1", default-features = false }
```

The lockfile is part of the qualification: do not change the version, checksum, feature set, or Rust 1.96.1 toolchain independently. A Turso upgrade requires reviewing upstream compatibility and rerunning the complete adapter conformance, migration, crash, WAL, backup/restore, semantic, server, and desktop suites. Treat a pre-release update as an engine migration, not a routine dependency refresh.

Unsupported modes include cloud/sync, PostgreSQL wire mode, default `mimalloc`/FTS features, experimental multiprocess/MVCC, and opening the live directory with SQLite, libSQL, or any other engine/tool.

## Paths, ownership, and permissions

`Config::new(database_directory, backup_root)` requires two distinct, non-overlapping, absolute normalized UTF-8 paths. Each is a dedicated directory, not a parent shared with unrelated files. On Unix, final directories must be owned by the current user and have no group/other permission bits (normally mode `0700`). Relative paths, traversal, symlink components, unsafe ownership/modes, and overlap are rejected.

Exactly one process may own a database directory. The advisory lock rejects a second cooperative owner; it is not multiprocess database support or protection against every same-UID namespace replacement attack. Local and network/distributed filesystems are not qualified. Keep both roots on trusted local storage and never cloud-sync the live directory.

The database filename and WAL/sidecars are adapter details. Do not copy or manipulate an individual file while running.

## Startup, migrations, and shutdown

Production startup is:

1. validate paths and acquire exclusive ownership;
2. open one local Turso engine, one serialized writer, and the bounded reader pool (four readers by default);
3. run bundled forward-only migrations;
4. validate or establish durable server/stream identity;
5. only then allow the server to bind and start its scheduler.

Migration ledger entries contain immutable version, name, and SHA-256 checksum. Unknown, too-new, duplicate, renamed, or checksum-drifted migrations fail startup. Migration DDL and its ledger insertion share an immediate transaction. A migration failure must leave the listener and scheduler stopped; there is no downgrade path.

Shutdown changes `Open` to `Draining`, rejects new work, waits for admitted work, drops writer/read connections and engine handles, and finally releases ownership. Stop listeners, subscriptions, scheduler, workers, and outstanding transactions before stopped maintenance. There is no promise that dropping a future synchronously cancels an engine operation or an in-progress commit.

## Transaction and cursor contract

Semantic mutations serialize through immediate transactions. Accepted mutations atomically commit authoritative state, idempotency outcome, immutable versioned ChangeEvent/history, Inbox effects, and an optional Outbox intent. Failed guards commit none of them. Publication is a server action after this commit; ChangeEvent is not delivery authority.

Snapshot roots and their cursor are read in one deferred transaction. Changes are read strictly after a cursor and classify invalid/expired/future positions explicitly. `server_id` and `stream_id` are durable database identity; a process boot is not a new stream. Restoring an older tail can make a formerly valid client cursor future, which must produce a gap and force a fresh snapshot.

Source occurrence uniqueness and mutation identities are database-enforced. An unknown commit outcome is `CommitOutcomeUnknown`, never an invitation to blindly replay. Resolve it using the stable mutation/occurrence identity: matching content may be recognized as committed, changed canonical content is a conflict, definite absence may permit a separately considered retry.

Outbox rows are the sole external-send authority. Delivery leases are fenced by token; success retains the provider message ID. Delivery is at-least-once: provider acceptance before durable success commit remains an unavoidable duplicate window.

## WAL operations

`cacheflush` writes dirty pages to WAL; it is neither a checkpoint nor a backup boundary. The pinned API exposes no supported high-level local checkpoint operation. Qualification observed both the nominal database and `-wal` file under load. The operational investigation threshold is 64 MiB for any regular database file: drain and inspect/restart or take a stopped backup rather than assuming online checkpointing.

## Stopped backup and restore

Backups are supported only after `AttentionDatabase::close()` reaches `Closed` and all server work has stopped. `backup(name)` reacquires ownership, copies every regular database-directory file except the adapter lock into an owner-only staging directory, syncs files/directories on Unix, writes a manifest, and atomically publishes the backup directory. Never back up only the nominal database file.

The manifest binds adapter/application compatibility, exact Turso version, migration head/checksum, sorted relative inventory, sizes, and SHA-256 checksums. Preserve the complete backup directory unchanged.

Restore procedure:

1. stop the server and verify no owner remains;
2. select a complete backup under the configured backup root;
3. provide an empty, validated destination database directory;
4. call `AttentionDatabase::restore(config, name)`;
5. let restore validate compatibility, migration metadata, inventory, sizes, and hashes and then reopen normally;
6. start the server and verify its restored `server_id`/`stream_id`; clients whose cursor is beyond the restored tail must snapshot after the explicit future-cursor gap.

Restore refuses a nonempty destination. Backup/restore is not corruption repair, online backup, point-in-time recovery, or a power-loss guarantee. Preserve a failed database for investigation and restore a known-good stopped backup; do not edit it with an external SQL tool.

For the specifically diagnosed duplicate-current-fire migration failure, first take a stopped backup and follow [the bounded repair runbook](maintenance/repair_duplicate_current_reminder_fires.md). No general repair CLI ships.

## Failure boundaries

- Native typed `Busy`, `BusySnapshot`, and constraint errors are classified without parsing messages. Only outcomes known not to have committed may be retried as whole transactions.
- A commit invocation without a definite result is ambiguous. Stable identity resolution is required; automatic replay is forbidden.
- Dropped read/write futures quarantine their connection. Later sanitation/reconnection is promised, not synchronous engine cancellation.
- Read-only/full conditions are typed where upstream exposes them, but destructive quota and permission behavior remains platform-dependent.
- `NotAdb`/`Corrupt` fail closed. No automatic recovery or corruption repair is claimed.
- Path and backup defenses reject tested traversal, symlink, ownership, permission, malformed-manifest, unsafe-inventory, overlap, and nonempty-restore cases. They do not establish complete filesystem namespace safety against a hostile same-UID process.
- Process-kill tests do not establish a power-loss durability guarantee.
- Online backup, single-file backup, network filesystems, cloud sync, multiprocess access, mixed-engine access, and forced cancellation are unsupported.
