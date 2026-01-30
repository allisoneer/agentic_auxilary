# Thoughts Tool v1 to v2 Migration Guide

## Overview

Thoughts Tool v2 introduces a cleaner three-space architecture that replaces the personal/repository mount system with distinct spaces for thoughts, context, and references. This guide will help you migrate from v1 to v2.

## Key Changes

### 1. Three-Space Architecture
- **v1**: Personal and repository mounts with complex merging
- **v2**: Three distinct spaces:
  - `thoughts/` - Your personal workspace
  - `context/` - Team-shared documentation
  - `references/` - Read-only external repositories

### 2. Personal Configuration Deprecated
- **v1**: `~/.thoughts/config.json` for personal mounts
- **v2**: All configuration in repository's `.thoughts/config.json`
- Personal configs are now ignored with a deprecation warning

### 3. No More Optional Mounts
- **v1**: Mounts could be marked as optional
- **v2**: All configured mounts are required

## Automatic Migration

**As of v2.0, the tool automatically migrates v1 configurations to v2 on any write operation.**

When you run commands like `thoughts init`, `thoughts mount add`, or `thoughts references add` on a v1 repository, the tool will:

1. **Automatically migrate** your configuration to v2 format
2. **Create a timestamped backup** (only if you have non-empty mounts or rules)
   - Backup location: `.thoughts/config.v1.bak-YYYYMMDD-HHMMSS.json`
   - Preserves your original v1 config including any `rules` field
3. **Show a one-line message** confirming the migration
4. **Continue with your requested command**

### What Gets Migrated

- Mounts with `sync: none` → become references
- Mounts with paths starting with `references/` → become references
- All other mounts → become context mounts
- Custom `mount_dirs.repository` → becomes `mount_dirs.context`
- Descriptions → preserved where applicable
- **Rules field** → dropped (not supported in v2, but saved in backup)

### Migration Message

After migration, you'll see:
```
Upgraded to v2 config. A v1 backup was created if non-empty. See MIGRATION_V1_TO_V2.md
```

This message appears only once per repository during the first write operation after migration.

## Manual Migration (Optional)

In most cases, automatic migration is sufficient. However, if you want explicit control:

### Using the Migrate Command

```bash
# Preview what will be migrated (dry-run)
thoughts config migrate-to-v2 --dry-run

# Perform migration with confirmation
thoughts config migrate-to-v2 --yes
```

The explicit migrate command provides:
- Summary of what will be migrated
- Control over timing of migration
- Same backup behavior as automatic migration

### Manual Steps (Advanced)

If you prefer to manually create a v2 config:

1. **Review Your Current Configuration**
   ```bash
   thoughts config show
   ```

2. **Backup Your v1 Config** (optional, done automatically)
   ```bash
   cp .thoughts/config.json .thoughts/config.v1.backup.json
   ```

3. **Create v2 Configuration**

   Example v2 configuration:
   ```json
   {
     "version": "2.0",
     "mount_dirs": {
       "thoughts": "thoughts",
       "context": "context",
       "references": "references"
     },
     "thoughts_mount": {
       "remote": "git@github.com:yourname/work-thoughts.git",
       "sync": "auto"
     },
     "context_mounts": [
       {
         "remote": "https://github.com/team/docs.git",
         "mount_path": "team-docs",
         "sync": "auto"
       }
     ],
     "references": [
       "https://github.com/rust-lang/rust",
       "https://github.com/tokio-rs/tokio"
     ]
   }
   ```

4. **Validate Configuration**
   ```bash
   thoughts config validate
   ```

5. **Re-initialize Your Repository**
   ```bash
   thoughts init
   ```

## Migration Examples

### Example 1: Simple v1 Config
```json
// v1 config
{
  "version": "1.0",
  "mount_dirs": {
    "repository": "context",
    "personal": "personal"
  },
  "requires": [
    {
      "remote": "https://github.com/team/docs.git",
      "mount_path": "docs",
      "description": "Team documentation",
      "sync": "auto"
    },
    {
      "remote": "https://github.com/rust-lang/rust.git",
      "mount_path": "references/rust",
      "description": "Rust language reference",
      "sync": "none"
    }
  ]
}
```

Automatically migrates to:
```json
// v2 equivalent (in-memory)
{
  "version": "2.0",
  "mount_dirs": {
    "thoughts": "thoughts",
    "context": "context",
    "references": "references"
  },
  "thoughts_mount": null,
  "context_mounts": [
    {
      "remote": "https://github.com/team/docs.git",
      "mount_path": "docs",
      "sync": "auto"
    }
  ],
  "references": [
    "https://github.com/rust-lang/rust.git"
  ]
}
```

### Example 2: Personal Config Users

If you had personal mounts in `~/.thoughts/config.json`:
1. You'll see a deprecation warning on each command
2. Manually add needed repositories:
   - Work repos → Set as thoughts_mount in repo config
   - Shared docs → Add with `thoughts mount add`
   - References → Add with `thoughts references add`

## New Features in v2

### Work Organization
```bash
# Create branch-based work directory
thoughts work init

# Complete and archive work
thoughts work complete
```

### Reference Management
```bash
# Add reference repositories
thoughts references add https://github.com/rust-lang/rust

# Sync all references
thoughts references sync
```

## Troubleshooting

### Q: My personal mounts aren't showing up
A: Personal mounts are deprecated. Re-add them as context mounts or configure a thoughts_mount.

### Q: Where did my optional mounts go?
A: Optional mounts are removed. All configured mounts are now required.

### Q: Can I still use v1 config?
A: Yes, but v1 configs are automatically migrated to v2 on the first write operation. Read-only operations continue to work with v1.

### Q: What happens to my rules field?
A: The `rules` field is not supported in v2 and will be dropped during migration. If you have rules defined, they will be preserved in the backup file (`.thoughts/config.v1.bak-*.json`).

### Q: How do I set up my personal workspace?
A: Add a thoughts_mount to your repository config pointing to your work repository.

## Rollback

If you need to rollback to v1 after migration:

1. Find your backup file:
   ```bash
   ls -la .thoughts/config.v1.bak-*
   ```

2. Restore the backup:
   ```bash
   cp .thoughts/config.v1.bak-YYYYMMDD-HHMMSS.json .thoughts/config.json
   ```

3. Verify configuration:
   ```bash
   thoughts config show
   ```

Note: Rollback should only be necessary in rare cases. The v2 format is fully backward compatible for read operations.

## Need Help?

If you encounter issues during migration:
1. Check for backup files in `.thoughts/config.v1.bak-*.json`
2. Run `thoughts config show` to see current configuration
3. Use `thoughts config validate` to check configuration syntax
4. Review migration messages for specific guidance