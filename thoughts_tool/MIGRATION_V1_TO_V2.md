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

The tool automatically handles v1 configurations:
- Mounts with `sync: none` → become references
- Mounts with paths starting with `references/` → become references
- All other mounts → become context mounts
- Personal mounts → ignored (warning displayed)

## Manual Migration Steps

### 1. Review Your Current Configuration

Check your existing v1 config:
```bash
thoughts config show
```

### 2. Identify Mount Types

Categorize your existing mounts:
- Work repositories → Configure as thoughts_mount
- Team documentation → Keep as context mounts
- Reference code → Move to references list

### 3. Create v2 Configuration

Example v2 configuration:
```json
{
  "version": "2.0",
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

### 4. Re-initialize Your Repository

After creating v2 config:
```bash
thoughts init --force
```

This creates the new three-symlink structure.

### 5. Update Mounts

```bash
thoughts mount update
thoughts sync --all
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
A: Yes, v1 configs work through automatic migration, but consider updating to v2 format.

### Q: How do I set up my personal workspace?
A: Add a thoughts_mount to your repository config pointing to your work repository.

## Need Help?

If you encounter issues during migration:
1. Check the warning messages for specific guidance
2. Run `thoughts mount list` to see current mount status
3. Use `thoughts config validate` to check configuration syntax