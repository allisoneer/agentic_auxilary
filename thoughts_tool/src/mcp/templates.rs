// Compile-time embedded templates and guidance strings for MCP get_template

// Embedded markdown templates (adjacent templates/ directory)
pub const RESEARCH_TEMPLATE_MD: &str = include_str!("templates/research_template.md");
pub const PLAN_TEMPLATE_MD: &str = include_str!("templates/plan_template.md");
pub const REQUIREMENTS_TEMPLATE_MD: &str = include_str!("templates/requirements_template.md");
pub const PR_DESCRIPTION_TEMPLATE_MD: &str = include_str!("templates/pr_description_template.md");

// Guidance strings (kept short; in-code by design)
pub const RESEARCH_GUIDANCE: &str = "Stop. Before writing this document, honestly assess: have you actually researched enough?

- Did you check what already exists in this branch? (list_active_documents)
- Did you trace the relevant code paths and find the key files?
- Are there gaps in your understanding you could fill by exploring more?

If yes to any of these, go back and investigate. The template is not a form to fill out—it's a place to document what you've already discovered. Don't write until you've done the work.";
pub const PLAN_GUIDANCE: &str = "";
pub const REQUIREMENTS_GUIDANCE: &str = "";
pub const PR_DESCRIPTION_GUIDANCE: &str = "";
