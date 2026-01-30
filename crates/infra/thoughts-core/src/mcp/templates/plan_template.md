# [Feature/Task Name] Implementation Plan

## Overview

[Brief description of what we're implementing and why]

## Current State Analysis

[What exists now, what's missing, key constraints discovered]

### Key Discoveries:
- [Important finding with file:line reference]
- [Pattern to follow]
- [Constraint to work within]

## What We're NOT Doing

[Explicitly list out-of-scope items to prevent scope creep]

## What we ARE Doing

[An explicit list of goals for the plan. What the "end state" of the repo will look like after the plan is written, if it's finished successfully]

## Implementation Approach

[High-level strategy and reasoning]

## Testing Strategy
[ A list of task-specific testing strategies that will be required to ensure a clean and happy
codebase that reaches the goals defined above. Should include all sections, and explicitly state
why unit, integration, or manual testing aren't required if they are deemed not to be ]

### Unit Tests:
- [What to test]
- [Key edge cases]

### Integration Tests:
- [End-to-end scenarios]

### Manual Testing Steps:
1. [Specific step to verify feature]
2. [Another verification step]
3. [Edge case to test manually]


## Phase 1: [Descriptive Name]

### Overview
[What this phase accomplishes]

### Success criteria

[ What the state of the codebase will be after the phase is complete. Similar to the "What we ARE
Doing" section at the top, but explicitly zoned to this phase ]

### Changes Required:

#### 1. [Component/File Group]
**File**: `path/to/file.ext`
**Changes**: [Summary of changes]

```[language]
// Specific code to add/modify
```

### Tests Required:

[ If the phase requires tests to be written or modified to be able to properly do automatic
verification, then they should be added here ]

### Success Criteria:

#### Automated Verification:
[ A relevant list of automatically executable verification that deterministically displays that this
phase was successful ]
- [ ] Linting/format checks pass (run the "check" recipe via `tools_just_execute`; discover available recipes with `tools_just_search`)
- [ ] Tests pass (run the "test" recipe via `tools_just_execute`; discover additional test recipes with `tools_just_search` if needed)

#### Manual Verification:
[ Only include if manual verification is required or useful for any given phase. ]
- [ ] Feature works as expected when tested via UI
- [ ] Performance is acceptable under load
- [ ] Edge case handling verified manually
- [ ] No regressions in related features

---

## Phase 2: [Descriptive Name]

### Overview
[What this phase accomplishes]

### Success criteria

[ What the state of the codebase will be after the phase is complete. Similar to the "What we ARE
Doing" section at the top, but explicitly zoned to this phase ]

### Changes Required:

#### 1. [Component/File Group]
**File**: `path/to/file.ext`
**Changes**: [Summary of changes]

```[language]
// Specific code to add/modify
```

### Tests Required:

[ If the phase requires tests to be written or modified to be able to properly do automatic
verification, then they should be added here ]

### Success Criteria:

#### Automated Verification:
- [ ] [Automated check with command]
- [ ] [Another automated check]

#### Manual Verification:
- [ ] [Manual verification item]
- [ ] [Another manual verification]

---

## Leftover outliers or outstanding questions

[ Is there anything that isn't fully fleshed out? Anything left to question? Do we successfully
reach the intended goal expressed at the beginning over the course of all phases? Can say "None"
if none. ]
