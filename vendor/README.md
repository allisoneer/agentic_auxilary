# Vendored Foreign Workspaces

## OpenAI Codex

`vendor/codex/` is a mechanical source drop of the approved upstream OpenAI Codex tree.

- The Rust workspace root stays at `vendor/codex/codex-rs/`.
- This subtree is intentionally **not** part of the root Cargo workspace.
- Root formatting and TODO scanning are configured to skip vendored sources.
- Use the delegated root commands instead of wiring Codex into the main workspace:
  - `just codex-check`
  - `just codex-build`
  - `just codex-test` (best-effort)
  - `just codex-run -- ...`

Approved upstream exclusions for this source drop:

- `.git/`
- `codex-cli/`
- `sdk/typescript/`
- `sdk/python/`
- `sdk/python-runtime/`
- `package.json`
- `pnpm-lock.yaml`
- `pnpm-workspace.yaml`

Treat vendored Codex as foreign upstream source. Do not refactor it into the root workspace in place.
