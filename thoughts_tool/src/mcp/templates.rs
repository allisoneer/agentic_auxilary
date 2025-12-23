// Compile-time embedded templates and guidance strings for MCP get_template

// Embedded markdown templates (adjacent templates/ directory)
pub const RESEARCH_TEMPLATE_MD: &str = include_str!("templates/research_template.md");
pub const PLAN_TEMPLATE_MD: &str = include_str!("templates/plan_template.md");
pub const REQUIREMENTS_TEMPLATE_MD: &str = include_str!("templates/requirements_template.md");
pub const PR_DESCRIPTION_TEMPLATE_MD: &str = include_str!("templates/pr_description_template.md");

// Guidance strings (kept short; in-code by design)
pub const RESEARCH_GUIDANCE: &str = "Before you write your research document, make sure you actually have researched enough. Make sure there isn't anything else you can figure out by exploring more.";
pub const PLAN_GUIDANCE: &str = "";
pub const REQUIREMENTS_GUIDANCE: &str = "";
pub const PR_DESCRIPTION_GUIDANCE: &str = "";
