# CLAUDE.md — gpt5_reasoner (Developer Guide)

## 1) Purpose and Scope
This file guides contributors and Claude Code on how to work on gpt5_reasoner. It focuses on internals, development workflows, and where to change things. End-user usage, installation, and examples live in README.md. In short: gpt5_reasoner optimizes a task into an XML template (optimizer, configurable model) and then executes it with GPT-5 high reasoning (executor), available via CLI and MCP.

## 2) Quick Dev Commands and Workflows
- Default targets (silent, warnings treated as failures via hack/cargo-smart.sh):
  - make check | make test | make build | make all
- Output variants:
  - Normal: make check-normal | test-normal | build-normal
  - Verbose: make check-verbose | test-verbose | build-verbose
- Cargo equivalents: cargo clippy | cargo test | cargo build
- Env:
  - OPENROUTER_API_KEY required (OpenRouter)
  - OPTIMIZER_MODEL optional; precedence param > env > default
  - Logs: RUST_LOG=gpt5_reasoner=debug for detailed tracing
- CLI:
  - --dot-env to load .env from CWD
  - Subcommands: run, mcp
- MCP:
  - Run server: gpt5_reasoner mcp
  - Inspect schema: cargo run --example print_schema

## 3) Architecture Overview and Data Flow
Pipeline: user prompt + file metadata → optimizer (Claude family) → YAML groups + XML template → inject file contents → token check → executor (GPT-5 high) → result. The optimizer sees filenames/descriptions; the executor sees actual file contents. Directory expansion happens before optimization. Robustness: app-level network retries, template validation retries, strict group marker validation, and token limit enforcement.

## 4) Core Modules and Responsibilities
- src/lib.rs:
  - gpt5_reasoner_impl: orchestrates the pipeline; expands directories; normalizes and dedups file paths; auto-injects CLAUDE.md memories (env-gated); pre-validates UTF-8; enforces plan guards; selects models; does optimizer validation-retry; injects files; enforces token limits; executes GPT-5 with retry on transport errors.
- src/client.rs:
  - OrClient: OpenRouter client loader (OPENROUTER_API_KEY).
- src/optimizer/mod.rs:
  - build_user_prompt(): fills templates with files+prompt.
  - call_optimizer(): async-openai call with application-level retries; sets ReasoningEffort::High for gpt-5/gpt-oss models.
  - prompts.rs: system+user templates; includes plan structure.
  - parser.rs: nested fence-aware YAML/XML parsing; group marker validation; candidate selection for XML when fences nest.
- src/template/mod.rs:
  - inject_files(): replaces <!-- GROUP: name --> with <group> and file contents; loads concurrently; uses embedded plan structure for plan_template group.
- src/token.rs:
  - count_tokens/enforce_limit with o200k_base; limit 250k.
- src/errors.rs:
  - ReasonerError ↔ ToolError mapping; retryability classification for OpenAI errors.
- src/main.rs:
  - clap-based CLI and MCP server wiring via universal_tool_core.

## 5) MCP and CLI Integration
- MCP: #[universal_tool_router] exposes MCP tool reasoning_model with method request(prompt, directories?, files, prompt_type).
- CLI:
  - Subcommands: run (prompt, files_json, optional directories_json), mcp
  - --dot-env flag to load .env
- To add a new MCP method:
  - Extend impl Gpt5Reasoner with #[universal_tool] function
  - Rebuild and run examples/print_schema.rs to verify schema

## 6) Critical Design Constraints and Gotchas
- Primary task placeholder: In lib.rs, the final prompt replaces a hardcoded placeholder string from templates with the actual user prompt. If you change the placeholder text in optimizer templates, update the replacement string in lib.rs.
- GROUP marker policy: Exactly <!-- GROUP: name -->; parser validates; template replaces by exact match. Changing the format requires updating parser + tests.
- Plan guards (PromptType::Plan):
  - Files are auto-injected with plan_structure.md (pre-optimizer)
  - Optimizer output must have a plan_template group referencing plan_structure.md; executor guard inserts/repairs if missing.
- Directory expansion:
  - Hidden dirs pruned unless include_hidden=true; extension filter is case-insensitive and accepts both "rs" and ".rs"
  - Binary/non-UTF-8 files skipped
  - Paths normalized to absolute; dedup after normalize
- Error handling/retries:
  - Optimizer: app-level retries for transport; validation retry loop for template errors
  - Executor (GPT-5): app-level single retry for transient failures
- Token budget: 250k enforced after injection; change TOKEN_LIMIT in token.rs with matching tests.
- Temperature defaults 0.2; reasoning_effort only for gpt-5/gpt-oss.

## 7) Typical Changes and Where to Make Them
- Prompt wording/behavior: edit src/optimizer/prompts/*.md; keep labels (FILE_GROUPING, OPTIMIZED_TEMPLATE) and GROUP markers exact; update lib.rs placeholder if edited.
- Parser policy: src/optimizer/parser.rs when adjusting fences/labels; add tests for nested/mixed fences.
- Directory logic: edit ext_matches and expand_directories_to_filemeta in lib.rs; keep tests updated.
- Model selection: select_optimizer_model in lib.rs; see model_selection_tests.
- Token budget: TOKEN_LIMIT in token.rs + tests.
- MCP surface: add methods in impl Gpt5Reasoner; verify schema with examples/print_schema.rs.

## 8) Testing Strategy and Debugging Tips
- Run: make test (silent; warnings fail via cargo-smart.sh)
- Debug logs: RUST_LOG=gpt5_reasoner=debug to see optimizer raw output, fence stats, token counts.

### 8.1) Tests that mutate process-global state (env, cwd)

- Always use serial_test named lock: `#[serial(env)]`
- Never stack multiple EnvGuard sets in one test. Prefer separate tests or tightly scoped blocks.
- Use shared utilities from `crate::test_support::{EnvGuard, DirGuard}`
  - `EnvGuard::set("VAR", "value")` / `EnvGuard::remove("VAR")`
  - `DirGuard::set(tempdir.path())`
- Note: `std::env::set_var`/`remove_var` are `unsafe` in Rust because they can cause data races. The shared guards handle this correctly and are safe when used with `#[serial(env)]` which prevents concurrent execution.
- Example:

```rust
use serial_test::serial;
use crate::test_support::{EnvGuard, DirGuard};

#[test]
#[serial(env)]
fn my_env_test() {
    let _g = EnvGuard::set("MY_FLAG", "1");
    assert_eq!(std::env::var("MY_FLAG").unwrap(), "1");
}
```

- Template debugging:
  - Look for "Template validation failed" retries and marker diagnostics
  - Reproduce tricky outputs as parser unit tests
  - Check final injected prompt preview logs

## 9) Important Files to Know
- src/lib.rs: orchestration, guards, normalization, retries
- src/optimizer/parser.rs: fence-aware parsing and validation
- src/template/mod.rs: content injection and I/O
- src/prompts/*: prompt templates and embedded plan structure
- examples/*: directories.json, test_files.json, print_schema.rs

## 10) What to Exclude from this File
- Install and end-user usage (see README)
- Full CLI examples and directory JSON shapes (see README)
- Exhaustive DirectoryMeta documentation (see README)
- General product marketing; keep developer-focused
