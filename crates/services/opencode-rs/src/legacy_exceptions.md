# OpenCode v1.17.2 legacy exception ledger

This file is the authoritative local ledger for the only approved non-`/api/*`
OpenCode routes retained during the v1.17.2 migration.

## Required call-site convention

Every direct usage must carry a nearby comment in this form:

```rust
// LEGACY_EXCEPTION(OpenCode v1.17.2): <why this route is still required>
```

## Allowed legacy exceptions (ONLY)

1. `POST /session`
   - Planned usage:
     - `crates/services/opencode-rs/src/http/legacy/session.rs`
     - `apps/opencode-orchestrator-mcp/src/tools.rs`
   - Removal trigger:
     - Upstream adds a supported `/api/*` session-create route.

2. `GET /session/:sessionID`
   - Planned usage:
     - `crates/services/opencode-rs/src/http/legacy/session.rs`
     - `apps/opencode-orchestrator-mcp/src/tools.rs`
   - Removal trigger:
     - Upstream adds a supported `/api/*` session fetch-by-id route.

3. `POST /session/:sessionID/command`
   - Planned usage:
     - `crates/services/opencode-rs/src/http/legacy/session.rs`
     - `apps/opencode-orchestrator-mcp/src/tools.rs`
   - Removal trigger:
     - Upstream adds a supported `/api/*` command-execution route.

4. Startup-only `GET /global/health`
   - Planned usage:
     - `crates/services/opencode-rs/src/http/legacy/global.rs`
     - `apps/opencode-orchestrator-mcp/src/server.rs`
   - Removal trigger:
     - Upstream exposes exact version information on a supported `/api/*` startup probe.

## Explicitly disallowed legacy/compatibility surfaces

- `session.init`
- deprecated permission shim routes
- legacy `/session/status`
- legacy `/permission*`
- legacy `/question*`
- legacy `/provider` as the primary orchestrator runtime surface

## Review guidance

- If any new non-`/api/*` route is proposed, stop and get plan approval.
- When upstream parity lands, remove the route from the SDK, orchestrator call sites,
  and this ledger in the same change.
