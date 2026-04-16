# Configuration

There are three config surfaces worth caring about here: `agentic.toml` for non-secret agent/tool settings, `.thoughts/config.json` for per-repo mount intent, and `~/.config/agentic/repos.json` for URL-to-local-path mappings. They do different jobs, which is good, because mixing them would be worse.

## `agentic.toml` (global + local)

`agentic.toml` is the non-secret config layer used by `agentic-mcp`, the reasoning tool, the orchestrator, and a few other pieces. The merge order is defaults → global `~/.config/agentic/agentic.toml` → local `./agentic.toml` → environment variables.

Useful commands:

```bash
agentic config init
agentic config edit
agentic config validate
```

Minimal shape, trimmed from [`../agentic.toml.example`](../agentic.toml.example):

```toml
[subagents]
locator_model = "claude-haiku-4-5"

[reasoning]
executor_model = "openai/gpt-5.2"

[services.anthropic]
base_url = "https://api.anthropic.com"

[logging]
level = "info"
```

Think of this as orientation and defaults, not secret storage. Models, base URLs, timeouts, and logging belong here; API keys do not.

## `.thoughts/config.json` (per repo, v2 only)

This file lives at `<repo>/.thoughts/config.json` and tells `thoughts` what the repo wants mounted. Current runtime expects `version: "2.0"`; older v1 configs are not supported anymore.

Useful commands:

```bash
thoughts config show
thoughts config edit
thoughts config validate
```

Minimal shape:

```json
{
  "version": "2.0",
  "thoughts_mount": { "remote": "git@github.com:user/thoughts.git", "sync": "auto" },
  "context_mounts": [{ "remote": "https://github.com/team/docs.git", "mount_path": "team-docs", "sync": "auto" }]
}
```

The real file can also include `mount_dirs` and `references`, but the shape above is the part most people need to orient themselves: one optional thoughts repo, zero or more context repos, then a reference list.

## `~/.config/agentic/repos.json` (repo mappings)

This is the canonical mapping file for repo URLs to local paths. `~/.thoughts/repos.json` is only legacy input now, not the current home for the file.

Minimal shape:

```json
{
  "version": "1.0",
  "mappings": {
    "github.com/org/repo": { "path": "/path/to/repo", "auto_managed": false }
  }
}
```

`auto_managed` is the part that usually matters in practice. `true` means the tooling created/owns that clone path; `false` means you pointed the system at a repo you manage yourself.

If you came here from setup, the next useful stop is usually [`./auth.md`](./auth.md) for secrets or [`./troubleshooting.md`](./troubleshooting.md) when a mapping exists but the mount still is not doing what you expected.
