# Existing repo setup (repo already has Thoughts config)

Use this path when the repo already has `.thoughts/config.json` and you mostly need local state to catch up.

One nuance that matters more than it first appears: a lot of people do **not** want every thoughts/context/reference repo auto-managed for them. If you cloned something yourself because you want to edit it directly, inspect `git status`, keep it private, or just control where it lives, register that local clone **before** you run commands that auto-clone missing repos. The auto-clone paths are `thoughts references sync` and `thoughts mount update`; if there is no mapping yet, those commands will use the managed clone location under `~/.thoughts/clones/...` and record it as auto-managed in `~/.config/agentic/repos.json`.

For local context repos, `thoughts mount add /path/to/repo <mount_path>` is the clean CLI path: it records the local path with `auto_managed=false` and adds the mount to `.thoughts/config.json`. The broader rule is simple even if your setup is a little weirder than that: get your local mapping in place first, then let the sync/update commands reconcile around it.

## Setup steps

1. Make sure you are inside the repo you actually want to use.

   `thoughts init` only works inside a git repository; outside one it fails with `Not in a git repository. Run 'git init' first.`

2. Initialize the local checkout.

   ```bash
   thoughts init
   ```

   This refreshes the local Thoughts scaffolding for the repo and makes sure the symlinks and control state are in place for your current checkout.

3. If you already cloned a context repo yourself and want to keep using that clone, register it before any auto-clone command runs.

   ```bash
   thoughts mount add /path/to/repo <mount_path>
   ```

   This adds the context mount to config and stores the repo mapping as user-managed instead of auto-managed.

4. Sync configured references.

   ```bash
   thoughts references sync
   ```

   This clones or fast-forwards the configured reference repos and writes auto-managed mappings for anything it had to manage itself.

5. Reconcile the live mounts.

   ```bash
   thoughts mount update
   ```

   This is the FUSE reconciliation step: it compares desired state to active mounts, remounts what is missing, and is also the command you need after a reboot.

6. Optionally sync the git-backed thoughts/context mounts.

   ```bash
   thoughts sync --all
   ```

   This does git sync for auto-sync mounts. It is useful, but it is not a substitute for `thoughts mount update`.

If you need to change what is mounted, not just bring it up, read [`../config.md`](../config.md). If something still looks wrong after that, go straight to [`../troubleshooting.md`](../troubleshooting.md).
