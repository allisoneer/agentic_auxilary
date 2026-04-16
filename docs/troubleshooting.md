# Troubleshooting

This is the practical version. The goal here is not "list every flag," it is "get you back to a working state without guessing."

## Mounts are missing (usually after reboot)

If `thoughts/`, `context/`, or `references/` are empty or gone after a restart, run:

```bash
thoughts mount update
```

`thoughts sync` only does git sync for auto-sync mounts. It does **not** recreate the FUSE mounts.

## Inspecting mount state with `thoughts mount debug`

When `mount update` is not enough, use the debug subcommands instead of poking blind.

```bash
thoughts mount debug info <target>
thoughts mount debug command <mount_name>
thoughts mount debug remount <mount_name>
```

- `debug info` shows what the tool thinks is mounted, along with backend/mount metadata.
- `debug command` prints the exact mount command it would run for that mount.
- `debug remount` does the blunt thing: unmount, then mount again for one target.

If you just want to see the full surface first, `thoughts mount debug --help` is worth a quick glance.

## Reference clone failures vs mapping issues

These are two different classes of failure, and the commands are different too.

- `thoughts references sync` is where clone/update failures show up. If the repo cannot be cloned because of network, auth, or remote access problems, the command reports that inline as `Failed to clone` or `Failed to update`.
- `thoughts references doctor` is for local state problems after the fact: stale mappings, missing paths, non-directories, non-git directories, or origin mismatches.

Typical sequence:

```bash
thoughts references sync
thoughts references doctor
thoughts references doctor --fix
thoughts mount update
```

`doctor --fix` only applies safe local fixes. It can prune stale auto-managed mappings, but it does not fix a bad network connection and it does not delete clone directories for you.

## You wanted your own clone, but Thoughts made one anyway

That usually means the repo URL had no local mapping yet, so `references sync` or `mount update` fell back to the managed clone path under `~/.thoughts/clones/...`.

If you want to keep a repo in your own directory, register that local path before the auto-clone commands run. For local context repos, the documented CLI path is:

```bash
thoughts mount add /path/to/repo <mount_path>
```

The mapping then lands in `~/.config/agentic/repos.json` with `auto_managed=false`.

## `thoughts init` says you're not in a git repo

The error is literal:

```bash
git init
thoughts init
```

`thoughts init` requires a git repository. No repo, no setup.

## `agentic-mcp --list-tools` looks empty

The gotcha here is simple: `agentic-mcp --list-tools` writes to **stderr**, not stdout.

```bash
agentic-mcp --list-tools 2>&1
```

If config loading produced warnings, those are also printed to stderr before startup.

## Log locations and levels

There are a few layers here.

- `RUST_LOG` works directly for `agentic-mcp` because it uses `tracing_subscriber`'s normal env-driven setup.
- `AGENTIC_LOG_LEVEL` and `AGENTIC_LOG_JSON` are config env overrides for the `agentic.toml` logging section.
- `OPENCODE_ORCHESTRATOR_LOG_DIR` tells `opencode-orchestrator-mcp` exactly where to write its JSONL/markdown tool-call logs.

Examples:

```bash
RUST_LOG=debug agentic-mcp --list-tools 2>&1
AGENTIC_LOG_LEVEL=debug AGENTIC_LOG_JSON=1 agentic-mcp --list-tools 2>&1
OPENCODE_ORCHESTRATOR_LOG_DIR=/tmp/opencode-orchestrator-logs opencode-orchestrator-mcp
```

If `OPENCODE_ORCHESTRATOR_LOG_DIR` is unset, the orchestrator falls back to the active Thoughts logs directory when one is available.

## Orchestrator says OpenCode version is wrong

`opencode-orchestrator-mcp` expects OpenCode `v1.3.17`.

If the binary on `PATH` is not the one you want, set `OPENCODE_BINARY`. If you are using launcher mode (for example `bunx --yes opencode-ai@1.3.17`), also set `OPENCODE_BINARY_ARGS`.

## One small gotcha worth knowing

`thoughts status --detailed` currently does not add extra detail beyond `thoughts status`, so do not burn time expecting hidden output there.

If you need the setup flow again, go back to [`./setup/README.md`](./setup/README.md). If the problem is clearly credentials rather than mounts or mappings, jump to [`./auth.md`](./auth.md).
