# New repo setup (first time adding Thoughts)

This is the path for a repo that does **not** have `.thoughts/config.json` yet.

## Setup steps

1. Make the repo a git repository first.

   ```bash
   git init
   ```

   `thoughts init` requires a git repo. If you skip this, it fails immediately and tells you to run `git init` first.

2. Initialize Thoughts.

   ```bash
   thoughts init
   ```

   This creates the local Thoughts control files and the `thoughts/`, `context/`, and `references/` symlinks for the repo.

3. Edit the v2 config for the mounts you actually want.

   ```bash
   thoughts config edit
   ```

   This is where you set `thoughts_mount`, `context_mounts`, and `references`. If you want the file shapes and precedence rules first, read [`../config.md`](../config.md).

4. Add context mounts.

   ```bash
   thoughts mount add <url-or-path> <mount_path>
   ```

   This is the easiest way to add a team/shared repo to the `context/` tree while also recording its repo mapping.

5. Add reference repos and make sure they exist locally.

   ```bash
   thoughts references add <repo-url>
   thoughts references sync
   ```

   `references add` records what you want, and `references sync` clones or updates the configured references so there is something to mount.

6. Bring the mounts up.

   ```bash
   thoughts mount update
   ```

   This reconciles desired state against the live FUSE mounts and is the command that actually makes the spaces appear in the repo.
