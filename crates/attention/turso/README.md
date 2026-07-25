# attention-turso

`attention-turso` is the exact-pinned native local Turso foundation for Attention. It qualifies and owns local engine lifecycle, connection ownership, migrations, commit ambiguity, WAL/file behavior, and stopped backup/restore. It intentionally contains no production Attention domain schema or kernel port implementation.

## Supported contract

- Turso is pinned exactly to `0.8.0-pre.1` with default features disabled. Cloud/sync, PostgreSQL wire mode, default `mimalloc`/`fts`, mixed SQLite access, and experimental multiprocess/MVCC modes are unsupported.
- One process exclusively owns one absolute, normalized, owner-only dedicated database directory. Database and backup roots reject traversal, symlink components, overlap, unsafe permissions/ownership, and attacker-selected destinations.
- Lifecycle is `Opening -> Open -> Draining -> Closed`. Operations acquire lifecycle permission before writer/read permission. Draining rejects new work, waits for active work, drops all connections before the engine, then releases directory ownership.
- One unexposed writer connection serializes immediate transactions. Four independently connected readers are the shipped bound and deferred transactions provide snapshots. No adapter code clones a Turso `Connection`.
- The shipped busy timeout is 250 ms (`BusyTimeout::Standard`). Retained barriers prove a second writer remains queued for at least 25 ms, at most four readers enter, and a writer completes within one second while two long snapshots are held.
- A dropped writer or reader future removes its connection from the reusable set. Later work independently reconnects. Sanitation drives exact-tag deferred rollback cleanup with a query and proves a new transaction can begin and roll back before reuse.
- Bundled startup-only migrations are forward-only. The ledger records immutable version, name, and SHA-256 checksum. Open/reopen rejects duplicate, unknown, too-new, renamed, or drifted state; migration DDL and ledger insertion share one immediate transaction.
- A failure after commit invocation without a definite result is `CommitOutcomeUnknown`. It is never replayed automatically. Reopen resolves the stable qualification identity as matching committed value, definitely absent, identity conflict, or a terminal read/integrity error. Only definitely absent permits a separately evaluated new attempt.

## WAL and stopped operations

The retained workload holds a deferred reader while committing 512 independent 4 KiB writes, flushes dirty pages, and reopens three times. The exact-tag regular-file inventory observed during the held snapshot is the nominal database plus `-wal` (`wal.db`, `wal.db-wal`). The production operation therefore copies every regular file in the dedicated directory except the adapter lock file; it never assumes the nominal file is sufficient.

The operational threshold is 64 MiB for any regular database file under this workload (`WAL_OPERATIONAL_LIMIT_BYTES`). Crossing it requires draining and investigating/restarting or taking a stopped backup; the adapter does not claim an unavailable online checkpoint primitive.

Backup is accepted only from `Closed`. It reacquires directory ownership, copies the complete regular-file set without following links into an owner-only staging directory, syncs files/directories on Unix, writes a checksummed compatibility manifest, and atomically publishes the completed backup directory. Restore validates the exact adapter/Turso versions, migration head/checksum, sorted relative inventory, sizes, and SHA-256 checksums, then copies only into an empty validated directory and reopens through the adapter.

## Exact-version limitations

The pinned local API exposes no high-level close, engine cancellation/interrupt, migration, backup, or local checkpoint primitive. Dropping a transaction defers rollback until later connection use, so this crate promises adapter-visible quarantine and sanitation rather than synchronous engine cancellation. `cacheflush` writes dirty pages to WAL and is not a checkpoint or backup boundary. Online backup is unsupported. Full/quota and filesystem permission fixtures are platform-dependent; typed policies are retained without parsing upstream error messages, while destructive process-kill, corruption, symlink, and Unix permission tests run on the current Linux qualification platform.
