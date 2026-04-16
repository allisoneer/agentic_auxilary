# Troubleshooting

## Thoughts: mounts missing (often after reboot)

**Symptom:** `thoughts/`, `context/`, or `references/` look empty or missing.

**Fix:**

```bash
thoughts mount update
```

`thoughts sync` does git sync. It does not recreate mounts.

## Thoughts: `Not in a git repository`

**Symptom:** `thoughts init` fails with `Not in a git repository. Run 'git init' first.`

**Fix:**

```bash
git init
thoughts init
```

## References: repos not mounted

**Fix:**

```bash
thoughts references sync
thoughts mount update
```

## References mapping cleanup

If reference mappings are stale or duplicated:

```bash
thoughts references doctor
thoughts references doctor --fix
```

`doctor --fix` cleans stale auto-managed mappings, but it does not delete clone directories.

## agentic-mcp: `--list-tools` shows nothing

**Gotcha:** `agentic-mcp --list-tools` prints to **stderr**, not stdout.

Try:

```bash
agentic-mcp --list-tools 2>&1 | sed -n '1,120p'
```

## Orchestrator: OpenCode version mismatch

`opencode-orchestrator-mcp` expects OpenCode v1.3.17.

If you are not using the default pinned binary path, set:

- `OPENCODE_BINARY`
- `OPENCODE_BINARY_ARGS`

## More docs

- Setup entrypoint: [`./setup/README.md`](./setup/README.md)
- Configuration: [`./config.md`](./config.md)
- Authentication: [`./auth.md`](./auth.md)
