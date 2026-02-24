//! Configuration utilities for gpt5-reasoner.
//!
//! Model configuration is now handled via agentic_config::types::ReasoningConfig.
//! This module is kept for any future config-related utilities.

// NOTE: select_optimizer_model was removed. Model selection now comes from
// ReasoningConfig passed through the tool registry.

#[cfg(test)]
mod tests {
    // All model selection tests have been moved to use ReasoningConfig.
    // Reasoning effort parsing tests are in orchestration.rs.
}
