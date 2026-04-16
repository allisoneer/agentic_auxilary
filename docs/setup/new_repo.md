# New repo setup (first time adding Thoughts)

This is the path for a repo that does **not** have `.thoughts/config.json` yet.

## Setup steps

1. Make the repo a git repository first.

   ```bash
   git init
   ```

   `thoughts init` requires a git repo. If you skip this, it fails immediately and tells you to run `git init` first. That step does not require a remote yet.

2. Initialize Thoughts.

   ```bash
   thoughts init
   ```

   This creates the local Thoughts control files and the `thoughts/`, `context/`, and `references/` symlinks for the repo.

3. Edit the v2 config for the mounts you actually want.

   ```bash
   thoughts config edit
   ```

   This opens `.thoughts/config.json` in `VISUAL`, then `EDITOR`, then `vi`. When you exit, `thoughts` validates the file, rewrites it in normalized JSON, and updates mounts. If you prefer, you can edit `.thoughts/config.json` manually and then run `thoughts config validate` or `thoughts mount update` yourself. If you want the file shapes and precedence rules first, read [`../config.md`](../config.md).

4. Add context mounts.

   ```bash
   thoughts mount add https://github.com/team/docs.git <mount_path>
   ```

   This is the easiest way to add a team/shared repo to the `context/` tree while also recording its repo mapping. If you use a local path instead (for example `thoughts mount add /path/to/repo <mount_path>`), that local repo must already have an `origin` remote. Missing mappings can be created automatically later for URL-backed mounts, but local-path registration is keyed from the repo's canonical remote identity.

5. Add reference repos and make sure they exist locally.

   ```bash
   thoughts references add <repo-url>
   thoughts references sync
   ```

   `references add` records what you want, and `references sync` clones or updates the configured references so there is something to mount. If you use `thoughts references add /path/to/repo` instead of a URL, that local repo also needs an `origin` remote.

6. Bring the mounts up.

   ```bash
   thoughts mount update
   ```

   This reconciles desired state against the live FUSE mounts and is the command that actually makes the spaces appear in the repo.
