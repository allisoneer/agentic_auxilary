# OpenCode upgrade: 1.15.7 → 1.17.4

## Summary

- `opencode_rs` and `opencode-orchestrator-mcp` now require exact OpenCode `1.17.4` after optional `v` prefix normalization.
- The orchestrator still uses the legacy endpoints that upstream `1.17.4` continues to serve.
- The Rust SDK now also exposes a parallel V2 surface through `client.v2()` for `/api/*` endpoints.
- Mise-managed local iteration pins remain on `1.15.7` by explicit exception.

## What changed

### Exact version pin

The supported runtime target is now exactly `1.17.4` everywhere in this repo except the mise pin files.

Examples:

```bash
bun install -g opencode-ai@1.17.4
OPENCODE_BINARY=bunx OPENCODE_BINARY_ARGS="--yes opencode-ai@1.17.4"
```

### Legacy orchestrator behavior is preserved

This upgrade does **not** force the orchestrator onto `/api/*` routes. Existing orchestrator hot paths still rely on the legacy endpoints for:

- session lifecycle
- prompt/command dispatch
- permission handling
- question handling
- startup health/version validation

That keeps the orchestrator runtime stable while the repo adds additive V2 SDK support in parallel.

### New parallel V2 SDK surface

The SDK now exposes a separate V2 client:

```rust
use opencode_rs::ClientBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = ClientBuilder::new()
        .base_url("http://127.0.0.1:4096")
        .directory("/path/to/repo")
        .build()?;

    let health = client.v2().health().get().await?;
    let location = client.v2().location().get().await?;
    let models = client.v2().model().list().await?;

    println!("healthy={}", health.healthy);
    println!("directory={:?}", location.directory);
    println!("models={}", models.data.len());
    Ok(())
}
```

Implemented V2 groups in this upgrade:

- core: health, location, session, message, model, provider, permission, question
- additive optional: connector, fs (`list`/`find` only), reference

Explicitly deferred in this slice:

- `/api/fs/read/*` raw binary reads
- `/api/event` SSE streaming
- `/api/permission/saved*`

## V2 location semantics

Legacy SDK requests inject flat `directory` / `workspace` query params.

The new V2 transport does **not** reuse that behavior. Instead it sends V2 location context as:

- `location[directory]`
- `location[workspace]`

and expects V2 envelope differences such as:

- `{ data, cursor }` for list responses
- `{ location, data }` for location-wrapped responses

## Running live checks locally

Ignored integration tests still exist, but they remain environment-gated and are **not** a reliable CI gate today.

SDK live example:

```bash
OPENCODE_BINARY=bunx \
OPENCODE_BINARY_ARGS="--yes opencode-ai@1.17.4" \
OPENCODE_INTEGRATION=1 \
cargo test -p opencode_rs --features server --test integration -- --ignored
```

Orchestrator live example:

```bash
OPENCODE_BINARY=bunx \
OPENCODE_BINARY_ARGS="--yes opencode-ai@1.17.4" \
OPENCODE_ORCHESTRATOR_INTEGRATION=1 \
cargo test -p opencode-orchestrator-mcp --test integration -- --ignored --test-threads=1
```

## Verification limits

- CI still does **not** prove end-to-end OpenCode compatibility because the live integration suites are ignored and env-gated.
- This upgrade focused on deterministic unit/wiremock coverage plus crate/workspace checks.
- If you hit a live OpenCode mismatch, re-run the ignored tests locally against `opencode-ai@1.17.4` first.

## Mise exception

The following remain intentionally pinned to `1.15.7` for local mise-managed iteration:

- `crates/meta/xtask/src/mise.rs`
- generated `mise.toml`

That exception is deliberate and should not be treated as an upgrade miss.
