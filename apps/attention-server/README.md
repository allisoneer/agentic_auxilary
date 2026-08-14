# Attention Server

`attention-server` is the bounded Axum WebSocket composition for Attention protocol v1. It opens native local Turso storage, runs migrations, establishes durable identity, serves `/v1/ws`, publishes committed changes, owns one reminder scheduler, and exposes generic Outbox worker RPCs. It does not embed provider integrations.

## Start

The binary reads:

| Variable | Required | Meaning |
| --- | --- | --- |
| `ATTENTION_DATABASE_DIRECTORY` | yes | dedicated absolute owner-only local database directory |
| `ATTENTION_BACKUP_DIRECTORY` | yes | distinct dedicated absolute owner-only backup root |
| `ATTENTION_BIND` | no | socket address; default is an ephemeral IPv4 loopback port (`127.0.0.1:0`) |
| `ATTENTION_ALLOW_NON_LOOPBACK` | no | presence explicitly permits a non-loopback bind |
| `ATTENTION_MAX_SOURCE_COMPONENT_BYTES` | no | override the source identity-component byte limit |
| `ATTENTION_MAX_SOURCE_ORDER_BYTES` | no | override the source-order byte limit |

Example development startup with a stable port:

```bash
install -d -m 0700 "$HOME/.local/share/attention/database" "$HOME/.local/share/attention/backups"
ATTENTION_DATABASE_DIRECTORY="$HOME/.local/share/attention/database" \
ATTENTION_BACKUP_DIRECTORY="$HOME/.local/share/attention/backups" \
ATTENTION_BIND=127.0.0.1:8787 \
cargo run -p attention-server
```

The two paths must satisfy the ownership/security contract in [`attention-turso`](../../crates/attention/turso/README.md). They may not overlap. Do not run two servers against one directory or open it with SQLite/libSQL tooling.

Startup validates configuration, acquires storage ownership, opens Turso, runs forward migrations, loads/creates durable `server_id` and `stream_id`, and only then binds the listener and starts exactly one scheduler. Path, ownership, migration, identity, or bind failure prevents serving. There is no migration downgrade.

The CLI currently exposes only the variables above. Other `ServerConfig` bounds and browser origin allowlists are library-embedding configuration, not environment variables.

## Network and security boundary

The default is loopback-only. A non-loopback address is rejected unless `ATTENTION_ALLOW_NON_LOOPBACK` is present. That switch is an explicit exposure decision, **not** authentication, TLS, or public-internet hardening. This local MVP has no authentication or authorization; keep it on a trusted host/network boundary.

Native WebSocket clients normally omit `Origin` and are accepted. If an `Origin` header is present, the exact absolute `http://` or `https://` origin must be in `ServerConfig::allowed_origins`; the default allowlist is empty, so browser-origin upgrades are denied. Allowed origins cannot contain credentials, path, query, fragment, or whitespace.

Logs must remain metadata-only and redacted. Do not log frames, canonical source content, delivery text, tokens, provider message IDs, filesystem contents, or credentials. Secret-shaped/oversized/deep input is rejected, but callers must not use protocol fields to transport provider secrets.

## Bounds

Default runtime limits are:

- 128 connections;
- 1 MiB WebSocket message and frame size;
- JSON depth 32 and 8,192 nodes;
- source identity component 256 bytes and source-order data 4,096 bytes;
- 32 in-flight requests per connection;
- outbound queue 128 and publication buffer 256;
- replay pages of 256;
- at most 256 delivery claims and 65,536 delivery-text bytes per request;
- scheduler batches of 256 every 250 ms, with error backoff capped at 5 seconds;
- 5-second connection/listener shutdown grace.

All configured limits must be nonzero and fit protocol fields. Capacity exhaustion is explicit: connection admission can return service unavailable; slow/overflowed clients are disconnected and must resume from their last applied cursor or take a snapshot. These limits bound server memory/work; they are not provider payload or batch APIs.

## State, publication, and gaps

Snapshots contain complete server views and a cursor from one storage snapshot. Resume returns events strictly after an acknowledged cursor. Invalid, expired, or future cursors and mismatched server/stream identity are explicit gaps requiring a fresh snapshot. `server_id` and `stream_id` survive ordinary restart; a restored older event tail may make a newer client cursor future.

Mutation state, stable outcome, ChangeEvent/history, Inbox effects, and optional Outbox intent commit atomically in Turso. Live publication occurs only after commit. If a publication is missed because of disconnect or process failure, snapshot/resume—not a mutation receipt—is the recovery authority.

The scheduler only evaluates explicit reminder schedules. Due dates do not create reminders. Firing creates a ReminderFire and may create an Outbox intent; it does not create an AttentionSignal or recurrence. Scheduler failures are logged and retried with bounded backoff.

Outbox is the sole external-send authority. Generic workers claim, inspect, renew, succeed, or fail intents using fenced lease tokens. ChangeEvents are not send work. Delivery is at-least-once: provider acceptance before durable success commit can result in a duplicate. The server supplies no provider client, secret store, provider cursor/rate control, or exactly-once guarantee.

## Shutdown and maintenance

Send interrupt (`Ctrl-C`/SIGINT) and wait for the process to exit. Runtime shutdown stops admission, closes pre-hello and upgraded clients, drains admitted request/service work within configured server grace, joins the scheduler, closes all database connections/engine handles, and releases storage ownership. It does not claim forced cancellation of a commit already inside the engine.

Only after clean exit may an operator perform stopped backup/restore. Follow the complete [Turso backup/restore procedure](../../crates/attention/turso/README.md): copy the complete manifested file set through the adapter, never a nominal database file; restore only into an empty validated directory; then restart and validate restored durable identity and future-cursor gap behavior. Online backup, cloud sync, mixed-engine repair, corruption repair, and multiprocess operation are unsupported.

## Failure and recovery limits

- Busy/constraint errors are typed; retry only operations known not to have committed.
- A lost mutation response after transmission is ambiguous. Clients must resolve it with stable idempotency identity and must not blindly replay.
- Corruption and migration incompatibility fail startup closed; preserve evidence and restore a known-good stopped backup. No repair promise is made.
- Process-kill tests do not establish power-loss guarantees.
- The service does not provide auth, TLS termination, public-internet safety, online backup, cloud sync, recurrence, hard deletion/purge, provider integrations, offline mutation queues, or multi-server synchronization.
