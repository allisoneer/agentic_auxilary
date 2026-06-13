# gwt-worktree

`gwt-worktree` is a Rust library for gwt-compatible git worktree management.

## v1 scope

- gwt-compatible config load/save for `~/.config/gwt/config.toml`
- exact `.gwt` base-path and `README.md` sentinel behavior
- worktree listing with synthesized main-worktree entries
- typed planning and execution for switch/create, remove, and gc
- trait-based integration points for remote refresh, remote delete, and PR lookup

## Compatibility promises

- preserve `repos.<git_dir>` config key semantics
- omit `default_repo` when unset
- use raw branch paths under `<repo>.gwt/`
- keep reversible encoded admin names for libgit2 worktree identities

## Non-goals

- no CLI in v1
- no shell integration or subprocess calls to `git`/`gh`
- no built-in network refresh or PR provider implementation
- no execution of `post_create_commands` or `clean_command`; they remain typed data
