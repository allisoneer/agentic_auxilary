# Attention Desktop

Attention Desktop is the local Tauri v2 shell for the Attention server, built with React, TypeScript, Vite, and Bun. Rust owns the sole `attention-client` supervisor and WebSocket; React receives sanitized presentation DTOs through a narrow Tauri bridge and never connects to the server or database directly.

## Prerequisites and commands

Use the repository-pinned **Bun 1.3.14**. Bun is the only supported JavaScript package manager: commit `bun.lock`, use frozen installs, and do not add npm/yarn/pnpm lockfiles.

```bash
cd apps/attention-desktop
bun install --frozen-lockfile
bun run check
bun run test
bun run build
bun audit
bun tauri dev
# Native binary verification; bundling is intentionally disabled:
bun tauri build --debug --no-bundle
```

Root `just` recipes run deterministic frontend checks but deliberately do not install dependencies or access the network. Run the frozen install first on a fresh checkout. Linux native development requires the Tauri v2 WebKitGTK and related system development libraries. Packaging, signing, installers, updater, and production distribution are deferred; `--no-bundle` verifies the application binary only.

## Server connection

Start `attention-server` first and set its WebSocket URL for the Rust backend:

```bash
ATTENTION_SERVER_URL=ws://127.0.0.1:8787/v1/ws bun tauri dev
```

If unset, the current development default is `ws://127.0.0.1:8787`. The value remains in Rust and is not exposed as a frontend DTO. The desktop has no embedded server or database lifecycle.

Each desktop process creates exactly one Rust supervisor. A fresh process requests a fresh snapshot; its cursor is intentionally not persisted across process restarts. Within the process, reconnect resumes strictly after the last snapshot/change that React successfully applied and acknowledged.

## Synchronization contract

The bridge subscribes before reading bootstrap state, orders buffered/replayed messages by sequence, applies each complete snapshot or ChangeEvent to the reducer, renders it, and only then acknowledges its cursor. Mutation receipts record pending outcome/correlation but **do not** directly change materialized WorkItems, signals, reminders, or Inbox. Matching committed ChangeEvents—with complete affected views and explicit Inbox effects—are authoritative.

Invalid, expired, or future cursors, stream identity changes, and local replay overflow reset the generation. The supervisor discards partial state, requests a fresh snapshot, and disables mutations until authoritative connected state returns. Never merge old-generation events into a replacement snapshot or infer removals/lifecycle rules from diffs.

The Rust supervisor bounds pending changes at 256 and bridge replay messages at 512. A renderer that cannot keep up recovers by snapshot rather than growing memory without limit.

## Inbox and supported mutations

Inbox is a projection with three independent branches:

- open WorkItems, with **Complete** and **Cancel** actions;
- unacknowledged AttentionSignals, with **Acknowledge** independent of source lifecycle;
- current ReminderFires, with **Acknowledge** and **Snooze** actions.

The MVP UI can create WorkItems (optional due, scheduled, and defer times), complete/cancel them, acknowledge signals, create one explicit absolute Reminder for a WorkItem or signal, acknowledge a fire, and snooze it to a new absolute trigger. Due dates do not create reminders. Acknowledging/snoozing a fire consumes that fire, not the Reminder; snooze creates a distinct replacement fire identity. There is no recurrence, generic reminder deletion, hard deletion, purge, provider UI, or offline mutation queue.

Human updates send the revision currently displayed. A stale revision or create conflict is presented as a structured conflict; the application does not silently rebase or use last-writer-wins. The user must wait for current authoritative state and deliberately retry. If a response is lost after transmission, the UI reports that the mutation outcome is unknown. It does not auto-replay; the subsequent event/snapshot is authoritative. Current desktop-generated idempotency identities are process-local, so this UI offers no cross-process ambiguous-mutation retry workflow.

## Security architecture

- `src-tauri` owns the one typed client, heartbeat/reconnect logic, cursor frontier, and mutation identity generation.
- `src/bridge.ts` is the only frontend importer of `@tauri-apps/api`.
- React must not use `fetch`, `WebSocket`, browser storage, arbitrary HTTP, or direct filesystem/database access.
- DTOs expose allowlisted presentation fields and sanitized error categories/messages; source/provider identities, Outbox details, tokens, provider messages, and internal errors stay in Rust.
- Render text/URLs as untrusted data. Do not add unsafe HTML or open arbitrary links.
- Keep the CSP self-only with IPC as the only connection target. The `main` capability grants only the registered desktop commands and event listen/unlisten; do not add shell, filesystem, HTTP, opener, clipboard, remote URL, arbitrary network, or additional window capability without a reviewed requirement.
- No credentials or database state belong in JavaScript, browser storage, frontend environment variables, logs, or IPC DTOs.

This is a local unauthenticated MVP client. Server loopback/network policy remains the operator's responsibility; the desktop does not add authentication, TLS, provider-secret storage, cloud synchronization, or multi-server coordination.

## Shutdown and recovery

Normal window/application shutdown closes the Rust supervisor and WebSocket. Server shutdown, transport loss, heartbeat failure, backpressure, and cursor gaps are surfaced as bounded status/issues and recovered through reconnect/resume or a fresh snapshot. Do not treat the desktop as a durable cursor store or backup mechanism. Database backup/restore is a stopped server operation documented in the server and Turso READMEs.
