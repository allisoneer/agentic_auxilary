# Research: [Topic]

## Source Request (verbatim)
> [Paste the user's original prompt/request exactly as given. No paraphrasing.]

## Scope and Bounds (facts only)
- [Explicit facts clarifying scope]
- [Constraints discovered (config, flags, permissions)]
- [Assumptions verified in code or docs; include file:line references]

## Branch Context
- Active research/plan/artifact documents scanned in this branch:
  - [path/to/doc1.md] — [1 sentence factual description]
  - [path/to/doc2.md] — [1 sentence factual description]
- External references considered:
  - [references/org/repo] — [1 sentence factual relevance]

## Summary of Findings
- [Bullet facts with file:line references when applicable]
- [Keep this section factual; recommendations appear in their dedicated section below]

## Detailed Findings

### Code Locations
- `path/to/file.ext:123-156` — [What is here, factual]
- `path/to/dir/` — [Directory role; include key files]

### Mechanisms and APIs
- [Endpoints, RPCs, handlers, CLI commands; list signatures, routes, inputs/outputs] (file:line)

### Configuration and Flags
- [Env vars, feature flags, config keys; default values and where set/read] (file:line)

### Data Models and Schemas
- [Types, DB schemas, migrations, invariants] (file:line)

### Tests and Examples
- [Unit/integration tests and example usages to follow] (file:line)

### Patterns and Conventions
- [Naming, directory structure, interface patterns followed elsewhere] (file:line)

### Constraints and Invariants
- [Concurrency, idempotency, auth, rate limiting, performance SLOs] (file:line)

## Code References (index)
- `path/to/fileA.ext:line-range` — [1 sentence factual description]
- `path/to/fileB.ext:line-range` — [1 sentence factual description]

## Open Facts and Gaps
- [List missing facts yet to be gathered; focus on factual gaps here]
- [Solution proposals go in the Recommendations section]

## Recommendations

### Targeted Approaches
Focused fixes that address the immediate problem with minimal scope.

1. [Approach name]: [Brief description of what it accomplishes]
   - **Tradeoffs**: [What you gain vs what you defer or limit]

2. [Approach name]: [Brief description]
   - **Tradeoffs**: [What you gain vs what you defer or limit]

### Comprehensive Approaches
Broader solutions that address root causes or adjacent concerns.

1. [Approach name]: [Brief description of what it accomplishes]
   - **Tradeoffs**: [What you gain vs what it costs in effort/complexity/time]

2. [Approach name]: [Brief description]
   - **Tradeoffs**: [What you gain vs what it costs in effort/complexity/time]

## Iteration Comments
- [Short notes like "Re-verify X after Y merges" or "Cross-check Z in tests"]

## Addendum and Handoff
- If more research is needed, create an addendum file: `[base]_additional.md` and cross-link both ways.
- Handoff to planning: In a new Agent chat, run:
  `/create_plan_init [path/to/this/research.md]`
