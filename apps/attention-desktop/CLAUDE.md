# CLAUDE.md - Attention Desktop

## Scope

This directory is the Tauri v2 + React desktop product. Bun 1.3.14 is the sole frontend package
manager; commit `bun.lock` and never add another JavaScript lockfile.

## Commands

```bash
bun install --frozen-lockfile
bun run check
bun run test
bun run build
bun tauri build --debug --no-bundle
```

## Boundaries

- Rust in `src-tauri` owns exactly one `attention-client` supervisor and the only server connection.
- `src/bridge.ts` is the sole frontend Tauri API importer.
- Never add direct frontend networking/storage or shell, filesystem, HTTP, opener, and remote URL
  capabilities.
- Preserve ordered apply-before-ack processing, bounded replay state, reset-on-gap semantics, and
  sanitized presentation DTOs.
- Root recipes do not install packages; dependency installation is explicit and frozen.
