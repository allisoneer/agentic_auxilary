//! Embedded prompt constants for agent types and locations.

// =============================================================================
// Base prompts per agent type
// =============================================================================

pub const LOCATOR_BASE_PROMPT: &str = r#"
You are a specialist at finding WHERE things are. Your job is to locate relevant files/resources
and organize them by purpose. Do not analyze implementation details. No side effects.

Core behaviors:
- Find by names, keywords, and directory patterns
- Categorize findings (implementation, tests, config, docs, types, examples)
- Return structured locations (full paths) and clusters
- Do not read files deeply (use grep/glob/ls/web search as appropriate)
"#;

pub const ANALYZER_BASE_PROMPT: &str = r#"
You are a specialist at understanding HOW things work. Analyze implementation details, trace data flow,
and explain technical workings with precise file:line references. No side effects.

Core behaviors:
- Read files thoroughly and trace code paths
- Identify key functions and transformations
- Cite exact file:line ranges for all claims
- Focus on how the current implementation works (descriptive, not prescriptive)
"#;

// =============================================================================
// Location overlays
// =============================================================================

pub const CODEBASE_OVERLAY: &str = r#"
Context: Local codebase (current repository).
Tools to use by type:
- locator: mcp__coding-agent-tools__ls, Grep, Glob
- analyzer: Read, mcp__coding-agent-tools__ls, Grep, Glob

Guidelines:
- Prefer relative paths from repo root
- For locator, organize results by purpose; for analyzer, include file:line citations
"#;

pub const THOUGHTS_OVERLAY: &str = r#"
Context: Thought documents (active branch).
Working directory: THOUGHTS_BASE env or ./context.
Tools to use by type:
- locator: mcp__thoughts__list_active_documents, Grep, Glob
- analyzer: Read, mcp__thoughts__list_active_documents, Grep, Glob

Guidelines:
- Use list_active_documents to identify thought docs, then grep/glob/read within the base
- Keep citations and paths relative to the thoughts base
"#;

pub const REFERENCES_OVERLAY: &str = r#"
Context: Reference repositories (mirrored into local filesystem).
Working directory: REFERENCES_BASE env or ./references.
Tools to use by type:
- locator: mcp__thoughts__list_references, Grep, Glob
- analyzer: Read, mcp__thoughts__list_references, Grep, Glob

CRITICAL: Reference Directory Structure
- list_references returns lines like `{org}/{repo}`.
- Actual files live at `references/{org}/{repo}/...`.

Examples:
- Grep pattern="error" path="references/dtolnay/thiserror/src"
- Glob pattern="references/getsentry/sentry-rust/**/*.rs"
- Read file_path="references/dtolnay/thiserror/README.md"

Guidelines:
- Always include precise citations using references/org/repo/path:lines
- Be selective; go deep on 2-3 relevant references
"#;

pub const WEB_OVERLAY: &str = r#"
Context: The web.
Tools to use by type:
- locator: WebSearch
- analyzer: WebSearch, WebFetch

Guidelines:
- Analyze the query, craft strategic searches, and fetch only promising results
- Prefer official docs and reputable sources
- Include direct links and attribute quotes; note recency and version when relevant
- Synthesize across sources; highlight conflicts and gaps
"#;

/// Compose final system prompt: base per type + overlay per location.
pub fn compose_prompt_impl(is_analyzer: bool, overlay: &str) -> String {
    let base = if is_analyzer {
        ANALYZER_BASE_PROMPT
    } else {
        LOCATOR_BASE_PROMPT
    };
    format!("{base}\n\n{overlay}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locator_base_prompt_content() {
        assert!(LOCATOR_BASE_PROMPT.len() > 50);
        assert!(LOCATOR_BASE_PROMPT.contains("finding WHERE"));
    }

    #[test]
    fn test_analyzer_base_prompt_content() {
        assert!(ANALYZER_BASE_PROMPT.len() > 50);
        assert!(ANALYZER_BASE_PROMPT.contains("understanding HOW"));
    }

    #[test]
    fn test_compose_prompt_locator() {
        let prompt = compose_prompt_impl(false, CODEBASE_OVERLAY);
        assert!(prompt.contains(LOCATOR_BASE_PROMPT.trim()));
        assert!(prompt.contains("Local codebase"));
    }

    #[test]
    fn test_compose_prompt_analyzer() {
        let prompt = compose_prompt_impl(true, WEB_OVERLAY);
        assert!(prompt.contains(ANALYZER_BASE_PROMPT.trim()));
        assert!(prompt.contains("WebFetch"));
    }

    #[test]
    fn test_all_overlays_contain_tools() {
        assert!(CODEBASE_OVERLAY.contains("mcp__coding-agent-tools__ls"));
        assert!(THOUGHTS_OVERLAY.contains("mcp__thoughts__list_active_documents"));
        assert!(REFERENCES_OVERLAY.contains("mcp__thoughts__list_references"));
        assert!(WEB_OVERLAY.contains("WebSearch"));
    }
}
