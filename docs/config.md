# Configuration

This repo uses a mix of per-project config and global config. Internal links below are relative so they work on GitHub and locally.

## `agentic.toml` (global + local)

- Global: `~/.config/agentic/agentic.toml`
- Local (project): `./agentic.toml`

Precedence (lowest to highest):

1. Defaults
2. Global file
3. Local file
4. Environment variables

Create, edit, and validate:

```bash
agentic config init
agentic config edit
agentic config validate
```

See the full example file: [`../agentic.toml.example`](../agentic.toml.example)

## Thoughts repo config: `.thoughts/config.json` (v2 only)

- Location: `<repo>/.thoughts/config.json`
- Current runtime expects `version: "2.0"`.
- Older v1 configs are not supported by the current runtime.

Useful commands:

```bash
thoughts config show
thoughts config edit
thoughts config validate
```

High-level shape:

- `version`
- `mount_dirs`
- `thoughts_mount`
- `context_mounts`
- `references`

## Repo mapping file: `~/.config/agentic/repos.json`

Thoughts maintains a URL-to-local-path mapping file here:

- Canonical path: `~/.config/agentic/repos.json`
- Legacy input (migration only): `~/.thoughts/repos.json`

High-level shape:

- `version`
- `mappings` keyed by repo identity to `{ path, auto_managed, last_sync? }`

`auto_managed` indicates whether tooling owns lifecycle updates for that mapping.

## Related docs

- Setup flows: [`./setup/README.md`](./setup/README.md)
- Authentication reference: [`./auth.md`](./auth.md)
- Troubleshooting: [`./troubleshooting.md`](./troubleshooting.md)
