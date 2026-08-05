# attention-turso

`attention-turso` is the exact-pinned native local Turso adapter for Attention. It owns local engine lifecycle, connection ownership, migrations, commit ambiguity, WAL/file behavior, stopped backup/restore, and the T05 production implementation of `AttentionReadPort` and `AttentionCommitPort`.

## Supported contract

- Turso is pinned exactly to `0.8.0-pre.1` with default features disabled. Cloud/sync, PostgreSQL wire mode, default `mimalloc`/`fts`, mixed SQLite access, and experimental multiprocess/MVCC modes are unsupported.
- One process exclusively owns one absolute, normalized, owner-only dedicated database directory. Database and backup path components are traversed and opened from an opened root descriptor with descriptor-relative, no-follow operations; ownership and owner-only mode validation apply to the final dedicated database or backup root descriptor. Ownership locks are opened and validated relative to the retained database-directory descriptor. Public path accessors remain normalized display and legacy-consumer paths, not evidence of current filesystem identity.
- Lifecycle is `Opening -> Open -> Draining -> Closed`. Operations acquire lifecycle permission before writer/read permission. Draining rejects new work, waits for active work, drops all connections before the engine, then releases directory ownership.
- One unexposed writer connection serializes immediate transactions. Four independently connected readers are the shipped bound and deferred transactions provide snapshots. No adapter code clones a Turso `Connection`.
- The shipped busy timeout is 250 ms (`BusyTimeout::Standard`). Retained barriers prove a second writer remains queued for at least 25 ms, at most four readers enter, and a writer completes within one second while two long snapshots are held.
- A dropped writer or reader future removes its connection from the reusable set. Later work independently reconnects. Sanitation drives exact-tag deferred rollback cleanup with a query and proves a new transaction can begin and roll back before reuse.
- Bundled startup-only migrations are forward-only. The ledger records immutable version, name, and SHA-256 checksum. Open/reopen rejects duplicate, unknown, too-new, renamed, or drifted state; migration DDL and ledger insertion share one immediate transaction.
- A failure after commit invocation without a definite result is `CommitOutcomeUnknown`. It is never replayed automatically. Reopen resolves the stable qualification identity as matching committed value, definitely absent, identity conflict, or a terminal read/integrity error. Only definitely absent permits a separately evaluated new attempt.
- T05 persists authoritative WorkItems, signals, Reminders with complete fire history, SourceReceipts, SourceEntities, mutation outcomes, immutable versioned ChangeEvents, stream head/floor, Inbox effects, and optional base Outbox intents. Every semantic mutation uses one serialized immediate transaction; failed guards write nothing.
- Point reads, snapshots, and changes-after use independent configured reader connections. Snapshot roots and cursor share one deferred transaction. Changes-after classifies Future, Expired, or valid pages from floor, head, and rows in one deferred transaction.
- ChangeEvent and prior-outcome JSON bytes are adapter-owned and versioned. Unknown or malformed versions fail closed, historical views are decoded from original stored bytes, and diagnostics report only record kind, version, and byte length.
- Delivery leases, delivery state/checkpoints, scheduler behavior, and workers remain T06 scope. Protocol DTOs, opaque cursor parsing, native-to-wire mapping, and server synchronization remain T08 scope. Startup composition and migration invocation remain T09 scope.

## WAL and stopped operations

The retained workload holds a deferred reader while committing 512 independent 4 KiB writes, flushes dirty pages, and reopens three times. The exact-tag regular-file inventory observed during the held snapshot is the nominal database plus `-wal` (`wal.db`, `wal.db-wal`). The production operation therefore copies every regular file in the dedicated directory except the adapter lock file; it never assumes the nominal file is sufficient.

The operational threshold is 64 MiB for any regular database file under this workload (`WAL_OPERATIONAL_LIMIT_BYTES`). Crossing it requires draining and investigating/restarting or taking a stopped backup; the adapter does not claim an unavailable online checkpoint primitive.

Backup is accepted only from `Closed`. It reacquires directory ownership, copies the complete regular-file set without following links into an owner-only staging directory, syncs files/directories on Unix, writes a checksummed compatibility manifest, and atomically publishes the completed backup directory. Restore validates the exact adapter/Turso versions, migration head/checksum, sorted relative inventory, sizes, and SHA-256 checksums, then copies only into an empty validated directory and reopens through the adapter.

For duplicate current reminder fires that block migration 0003, follow the [backup-first stopped-database repair runbook](maintenance/repair_duplicate_current_reminder_fires.md). No repair CLI or SQL shell ships.

## Exact-version limitations

The pinned local API exposes no high-level close, engine cancellation/interrupt, migration, backup, or local checkpoint primitive. Dropping a transaction defers rollback until later connection use, so this crate promises adapter-visible quarantine and sanitation rather than synchronous engine cancellation. `cacheflush` writes dirty pages to WAL and is not a checkpoint or backup boundary. Online backup is unsupported. Full/quota and filesystem permission fixtures are platform-dependent; typed policies are retained without parsing upstream error messages, while destructive process-kill, corruption, symlink, and Unix permission tests run on the current Linux qualification platform.

Descriptor retention hardens only adapter-owned directory creation/validation and ownership-lock opening. `database_file()` is still converted to a string for `Builder::new_local`, so Turso database, WAL, journal, and sidecar opens do not inherit retained-directory identity. Backup inventory, staging, copy, rename, restore, publication, removal, and directory synchronization also remain pathname-based. The advisory lock binds the opened lock-file inode; same-UID unlink or replacement of its name after acquisition is not namespace locking and remains outside this contract.
