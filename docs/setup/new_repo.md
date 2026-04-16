# New repo setup (first time adding Thoughts)

## Steps

1. Ensure the repo is a git repository:

```bash
git init
```

2. Initialize Thoughts:

```bash
thoughts init
```

3. Edit config (v2 only) as needed:

```bash
thoughts config edit
```

4. Add context mounts:

```bash
thoughts mount add <url-or-path> <mount_path>
```

5. Add references and clone them:

```bash
thoughts references add <repo-url>
thoughts references sync
```

6. Mount everything:

```bash
thoughts mount update
```

For configuration details, see [`../config.md`](../config.md).
