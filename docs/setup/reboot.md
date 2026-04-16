# After reboot: remounting and recovery

## The important distinction

- `thoughts mount update`
  - reconciles desired state versus active mounts
  - mounts what is missing and unmounts what was removed
  - this is the after-reboot command

- `thoughts sync`
  - performs git sync for auto-sync mounts
  - does not recreate FUSE mounts

## Steps after reboot

1. Recreate mounts:

```bash
thoughts mount update
```

2. Optional: sync git-backed mounts:

```bash
thoughts sync --all
```

If mounts fail to come up, see [`../troubleshooting.md`](../troubleshooting.md).
