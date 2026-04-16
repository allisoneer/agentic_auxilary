# After reboot: remounting and recovery

The short version is that FUSE mounts are process-level state. They do not survive a reboot just because the config file still exists. `thoughts mount update` re-establishes the live mounts from the desired state on disk.

## `mount update` vs `sync`

- `thoughts mount update`
  - reconciles desired state versus active mounts
  - mounts what is missing and unmounts what was removed
  - this is the after-reboot command

- `thoughts sync`
  - performs git sync for auto-sync mounts
  - does not recreate FUSE mounts

## After-reboot flow

1. Recreate the mounts.

   ```bash
   thoughts mount update
   ```

2. If you also want the git-backed thoughts/context repos refreshed, do that separately.

   ```bash
   thoughts sync --all
   ```

If `mount update` still leaves things empty, head to [`../troubleshooting.md`](../troubleshooting.md). That is usually a mount/backend issue, not a sync issue.
