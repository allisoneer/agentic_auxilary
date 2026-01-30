pub const SYSTEM_OPTIMIZER: &str = include_str!("../prompts/expert-prompt-engineer.md");
pub const USER_OPTIMIZE_REASONING: &str = include_str!("../prompts/optimize-reasoning-prompt.md");
pub const USER_OPTIMIZE_PLAN: &str = include_str!("../prompts/optimize-plan-prompt.md");
// TODO(2): Consolidate with apps/thoughts/src/mcp/templates/plan_template.md.
// Goal: single template location owned by thoughts (apps/thoughts), consumed by gpt5_reasoner.
pub const PLAN_STRUCTURE_TEMPLATE: &str = include_str!("../prompts/plan_structure.md");
