# Existing repo setup (repo already has Thoughts config)

Use this path if the repository already contains `.thoughts/config.json` and you're setting up a new machine or a new checkout.

## Steps

1. Ensure you are inside a git repository.
   - If you run `thoughts init` outside a git repo, you'll see:
     `Not in a git repository. Run 'git init' first.`

2. Initialize Thoughts in the repo:

```bash
thoughts init
```

3. Reconcile or remount configured mounts:

```bash
thoughts mount update
```

4. Sync git-backed mounts if needed. This is not the remount command:

```bash
thoughts sync --all
```

5. If references are configured, ensure they are cloned, then remount:

```bash
thoughts references sync
thoughts mount update
```

6. For configuration details, see [`../config.md`](../config.md).

If something fails, see [`../troubleshooting.md`](../troubleshooting.md).
