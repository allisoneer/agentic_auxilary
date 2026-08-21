# attention-client

`attention-client` is the typed, bounded, reconnecting WebSocket SDK for Attention protocol v1. It depends on `attention-protocol`, not the kernel, database, server implementation, or provider SDKs.

## Connect and close

```rust
use attention_client::{Client, ClientConfig};

let config = ClientConfig::new("ws://127.0.0.1:8787/v1/ws");
let (client, mut subscription) = Client::connect(config)?;
// Consume subscription.snapshots, subscription.changes, and subscription.issues.
// Observe client.status().
client.close().await?;
```

`connect` starts one asynchronous supervisor. Configuration bounds the command/event queues, pending requests, request timeout, heartbeat interval/timeout, and exponential reconnect range. The defaults are a snapshot subscription, 64 commands, 256 events, 32 pending requests, a 30-second request timeout, 15-second heartbeat with 10-second timeout, and 100 ms–5 second reconnect delay. Queue exhaustion is `Backpressure`, not an unbounded buffer.

Call `close()` and await it for an orderly WebSocket/supervisor shutdown. Dropping a handle alone is not the documented clean-close sequence.

## Hello, subscription, and acknowledgement

The supervisor performs v1 hello/version negotiation and exposes durable `server_id` and `stream_id` in `ConnectionStatus::Connected`. Configure `ClientConfig::subscription` with the protocol's:

- `None` for RPC only;
- `Snapshot` for a consistent state plus `after_cursor` followed by newer events;
- `Resume` for the same server/stream strictly after an acknowledged cursor.

A snapshot cursor is not a resume frontier until the consumer has successfully applied the complete snapshot and calls `acknowledge_snapshot(after_cursor)`. Likewise, apply each complete ChangeEvent—including its authoritative affected views and Inbox upserts/removals—before calling `acknowledge_cursor(event.cursor)`. Acknowledgements are local SDK checkpoints, not mutation requests.

The client validates delivered and monotonic acknowledgements. On reconnect it resumes only from the last successfully acknowledged frontier. Invalid, expired, or future cursors and stream mismatch are explicit gaps; status becomes `Gap`, the frontier is discarded, and the supervisor negotiates a fresh snapshot. Consumers may also call `request_fresh_snapshot()` to reset the same supervisor. Never merge a partial old stream into the replacement snapshot.

Heartbeat/transport/protocol/gap failures appear on the bounded `issues` channel, while current state is available through `status()`. Overflow or transport loss disconnects and uses the acknowledged resume frontier; it does not make unacknowledged application state durable.

## Requests, idempotency, and ambiguity

A JSON-RPC request ID correlates one transport request/response. It is not a mutation idempotency key, source occurrence ID, resource ID, event ID, cursor, fire ID, Outbox ID, lease token, or provider message ID.

Queries that fail before a response may be retried according to caller policy. Mutations are stricter: once transmission is attempted, send failure, disconnect, or timeout returns `ClientError::AmbiguousMutation`. The supervisor does **not** automatically replay it. Resolve an unknown outcome using the stable mutation identity and point reads/retry semantics of that method. Reusing an identity with identical canonical content may return `Replayed`; changed content is an idempotency mismatch. Generate and retain mutation identities outside a retry loop.

Errors remain distinct: peer RPC rejection, local protocol/encoding failure, transport failure, timeout, bounded backpressure, ambiguous mutation, invalid acknowledgement, configuration, and closed client. Do not parse display strings to recover semantics.

## Producer/source ingress

External producers use `source_occurrence_ingest` with typed protocol DTOs. A producer owns provider communication, credentials, rate limits, pagination/cursors, raw payload handling, and mapping. Send only bounded canonical Attention fields:

- stable source kind/instance and external entity identity;
- stable occurrence identity and receipt identity;
- canonical fingerprint/content and occurrence/ingest times;
- source ordering information where the provider supplies a comparable order;
- explicit source lifecycle and fresh-attention intent;
- a stable mutation idempotency key.

The database enforces occurrence uniqueness without read-before-write. The same occurrence and canonical content replays; changed canonical content for that occurrence is an error. Older, equal, missing, or incomparable ordering may be recorded receipt-only and must not be interpreted by the producer as a human-state overwrite. Never place provider credentials or secret-bearing raw payloads in identity/text fields; server bounds are defense in depth, not a secret transport.

This SDK has no provider-specific adapter, batch-ingress, cursor store, or secret store.

## Generic delivery worker

Provider adapters consume only durable Outbox authority through the generic methods:

1. `delivery_claim` claims bounded available intents until lease expiry;
2. `delivery_inspect` rechecks current authority/state before sending;
3. `delivery_renew` extends a live lease when necessary;
4. perform the external send using the provider adapter;
5. `delivery_succeed` records durable success and provider message ID, or use `delivery_fail_retryable` / `delivery_fail_terminal` with the current lease token.

Lease tokens fence stale workers. Outbox intent identity, lease token, and provider message ID are separate. ChangeEvents are never a second work queue. On restart or checkpoint replay, inspect durable delivery state before sending again.

Delivery is at-least-once. If the provider accepts a message but the worker cannot durably commit success, a later claim may send a duplicate. A provider message ID is evidence retained after success; it is not an exactly-once guarantee. Workers own provider-specific dedupe/nonce behavior if available, but this SDK makes no provider guarantee.

## Scope limitations

There is no offline mutation queue, blind mutation replay, provider integration, authentication policy, database access, or multi-server synchronization in this crate. The SDK's automatic behavior is transport reconnect, heartbeat, resume from acknowledged state, and fresh-snapshot recovery after an explicit gap.
