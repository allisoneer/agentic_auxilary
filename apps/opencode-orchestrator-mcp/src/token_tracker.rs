//! Token tracking for context limit safeguard.
//!
//! Monitors token usage during session runs and detects when the 80% threshold
//! is reached to trigger server-side summarization.

use opencode_rs::types::event::Event;
use opencode_rs::types::message::Part;
use opencode_rs::types::message::TokenUsage;

fn sat_u32(value: u64) -> (u32, bool) {
    if value > u64::from(u32::MAX) {
        (u32::MAX, true)
    } else {
        (value as u32, false)
    }
}

/// Tracks token usage during a session run to detect context limit threshold.
#[derive(Debug, Clone)]
pub struct TokenTracker {
    /// Provider ID from message events
    pub provider_id: Option<String>,
    /// Model ID from message events
    pub model_id: Option<String>,
    /// Context limit for the current model (from cached limits)
    pub context_limit: Option<u64>,
    /// Latest observed input token count
    pub latest_input_tokens: Option<u64>,
    /// Latest observed full token usage
    pub latest_tokens: Option<TokenUsage>,
    /// Flag indicating compaction/summarization is needed
    pub compaction_needed: bool,
    /// Threshold at which to trigger summarization (0.0 - 1.0)
    threshold: f64,
}

impl Default for TokenTracker {
    fn default() -> Self {
        Self::with_threshold(0.80)
    }
}

impl TokenTracker {
    /// Create a new token tracker with a custom compaction threshold.
    ///
    /// The threshold should be between 0.0 and 1.0 (e.g., 0.80 for 80%).
    pub fn with_threshold(threshold: f64) -> Self {
        Self {
            provider_id: None,
            model_id: None,
            context_limit: None,
            latest_input_tokens: None,
            latest_tokens: None,
            compaction_needed: false,
            threshold,
        }
    }

    /// Create a new token tracker with default threshold (80%).
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe an SSE event and update token tracking.
    ///
    /// The `context_limit_lookup` function is called to look up the context limit
    /// for a given (`provider_id`, `model_id`) pair from the cached limits.
    pub fn observe_event<F>(&mut self, ev: &Event, context_limit_lookup: F)
    where
        F: Fn(&str, &str) -> Option<u64>,
    {
        match ev {
            Event::MessageUpdated { properties } => {
                // Extract provider/model info
                if let Some(pid) = properties.info.provider_id.as_ref()
                    && let Some(mid) = properties.info.model_id.as_ref()
                {
                    self.provider_id = Some(pid.clone());
                    self.model_id = Some(mid.clone());
                    self.context_limit = context_limit_lookup(pid, mid);
                    // Recompute threshold if this event didn't carry tokens
                    if properties.info.tokens.is_none() {
                        self.recompute_flag();
                    }
                }

                // Extract token usage
                if let Some(tokens) = &properties.info.tokens {
                    self.observe_tokens(tokens);
                }
            }
            Event::MessagePartUpdated { properties } => {
                // Check for StepFinish with token info
                if let Some(part) = properties.part.as_ref()
                    && let Part::StepFinish {
                        tokens: Some(tokens),
                        ..
                    } = part
                {
                    self.observe_tokens(tokens);
                }
            }
            _ => {}
        }
    }

    /// Observe token usage and update threshold flag.
    pub fn observe_tokens(&mut self, tokens: &TokenUsage) {
        self.latest_input_tokens = Some(tokens.input);
        self.latest_tokens = Some(tokens.clone());
        self.recompute_flag();
    }

    pub fn to_log_token_usage(&self) -> (Option<agentic_logging::TokenUsage>, bool) {
        let Some(tokens) = &self.latest_tokens else {
            return (None, false);
        };

        let (prompt, prompt_saturated) = sat_u32(tokens.input);
        let (completion, completion_saturated) = sat_u32(tokens.output);
        let total_raw = tokens
            .total
            .unwrap_or_else(|| tokens.input.saturating_add(tokens.output));
        let (total, total_saturated) = sat_u32(total_raw);
        let (reasoning, reasoning_saturated) = sat_u32(tokens.reasoning);
        let saturated =
            prompt_saturated || completion_saturated || total_saturated || reasoning_saturated;

        (
            Some(agentic_logging::TokenUsage {
                prompt,
                completion,
                total,
                reasoning_tokens: (tokens.reasoning > 0).then_some(reasoning),
            }),
            saturated,
        )
    }

    /// Recompute the `compaction_needed` flag based on current state.
    fn recompute_flag(&mut self) {
        if let (Some(input), Some(limit)) = (self.latest_input_tokens, self.context_limit)
            && limit > 0
        {
            let ratio = input as f64 / limit as f64;
            if ratio >= self.threshold {
                self.compaction_needed = true;
                tracing::info!(
                    "Context limit threshold reached: {}/{} ({:.1}%)",
                    input,
                    limit,
                    ratio * 100.0
                );
            }
        }
    }
}

// Test-only helper methods
#[cfg(test)]
impl TokenTracker {
    /// Reset after compaction/summarization.
    pub fn reset_after_compaction(&mut self) {
        self.compaction_needed = false;
        self.latest_input_tokens = None;
        self.latest_tokens = None;
    }

    /// Get the current usage ratio (0.0 to 1.0+).
    pub fn usage_ratio(&self) -> Option<f64> {
        match (self.latest_input_tokens, self.context_limit) {
            (Some(input), Some(limit)) if limit > 0 => Some(input as f64 / limit as f64),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencode_rs::types::event::MessagePartEventProps;
    use opencode_rs::types::event::MessageUpdatedProps;
    use opencode_rs::types::message::MessageInfo;
    use opencode_rs::types::message::MessageTime;

    fn mk_token_usage(input: u64) -> TokenUsage {
        TokenUsage {
            total: None,
            input,
            output: 0,
            reasoning: 0,
            cache: None,
            extra: serde_json::Value::Null,
        }
    }

    fn mk_message_updated(
        provider_id: Option<&str>,
        model_id: Option<&str>,
        tokens: Option<TokenUsage>,
    ) -> Event {
        Event::MessageUpdated {
            properties: Box::new(MessageUpdatedProps {
                info: MessageInfo {
                    id: "msg-1".to_string(),
                    session_id: None,
                    role: "assistant".to_string(),
                    time: MessageTime {
                        created: 0,
                        completed: None,
                    },
                    agent: None,
                    format: None,
                    model: None,
                    system: None,
                    tools: std::collections::HashMap::new(),
                    parent_id: None,
                    model_id: model_id.map(str::to_string),
                    provider_id: provider_id.map(str::to_string),
                    path: None,
                    cost: None,
                    tokens,
                    structured: None,
                    finish: None,
                    extra: serde_json::Value::Null,
                },
                extra: serde_json::Value::Null,
            }),
        }
    }

    fn mk_message_part_step_finish(tokens: Option<TokenUsage>) -> Event {
        Event::MessagePartUpdated {
            properties: Box::new(MessagePartEventProps {
                session_id: None,
                message_id: None,
                index: None,
                part: Some(Part::StepFinish {
                    id: None,
                    reason: "done".to_string(),
                    snapshot: None,
                    cost: 0.0,
                    tokens,
                }),
                delta: None,
                extra: serde_json::Value::Null,
            }),
        }
    }

    #[test]
    fn triggers_compaction_at_80_percent() {
        let mut tracker = TokenTracker::new();
        tracker.context_limit = Some(1000);

        // 79.9% - should not trigger
        tracker.latest_input_tokens = Some(799);
        tracker.recompute_flag();
        assert!(!tracker.compaction_needed);

        // 80.0% - should trigger
        tracker.latest_input_tokens = Some(800);
        tracker.recompute_flag();
        assert!(tracker.compaction_needed);
    }

    #[test]
    fn does_not_trigger_without_limit() {
        let mut tracker = TokenTracker::new();
        tracker.latest_input_tokens = Some(10000);
        tracker.recompute_flag();
        assert!(!tracker.compaction_needed);
    }

    #[test]
    fn reset_clears_flag() {
        let mut tracker = TokenTracker::new();
        tracker.context_limit = Some(100);
        tracker.latest_input_tokens = Some(90);
        tracker.recompute_flag();
        assert!(tracker.compaction_needed);

        tracker.reset_after_compaction();
        assert!(!tracker.compaction_needed);
        assert!(tracker.latest_input_tokens.is_none());
    }

    #[test]
    fn usage_ratio_calculation() {
        let mut tracker = TokenTracker::new();
        tracker.context_limit = Some(1000);
        tracker.latest_input_tokens = Some(500);

        assert_eq!(tracker.usage_ratio(), Some(0.5));
    }

    #[test]
    fn observe_event_tokens_first_limit_later_triggers_compaction() {
        let lookup = |_: &str, _: &str| Some(1000);
        let mut tracker = TokenTracker::new();

        // Tokens arrive first via StepFinish, but no context_limit yet
        let ev_tokens = mk_message_part_step_finish(Some(mk_token_usage(800)));
        tracker.observe_event(&ev_tokens, lookup);
        assert!(!tracker.compaction_needed); // Can't trigger without limit

        // Model info arrives later without tokens
        let ev_limit = mk_message_updated(Some("provider-1"), Some("model-1"), None);
        tracker.observe_event(&ev_limit, lookup);

        // Should now trigger because 800/1000 = 80%
        assert!(tracker.compaction_needed);
    }

    #[test]
    fn observe_event_limit_first_tokens_later_triggers_compaction() {
        let lookup = |_: &str, _: &str| Some(1000);
        let mut tracker = TokenTracker::new();

        // Model info arrives first
        let ev_limit = mk_message_updated(Some("provider-1"), Some("model-1"), None);
        tracker.observe_event(&ev_limit, lookup);
        assert!(!tracker.compaction_needed); // No tokens yet

        // Tokens arrive later
        let ev_tokens = mk_message_part_step_finish(Some(mk_token_usage(800)));
        tracker.observe_event(&ev_tokens, lookup);

        // Should trigger because 800/1000 = 80%
        assert!(tracker.compaction_needed);
    }

    #[test]
    fn observe_event_combined_message_updated_event_triggers_compaction() {
        let lookup = |_: &str, _: &str| Some(1000);
        let mut tracker = TokenTracker::new();

        // Single event with both model info and tokens
        let ev = mk_message_updated(
            Some("provider-1"),
            Some("model-1"),
            Some(mk_token_usage(800)),
        );
        tracker.observe_event(&ev, lookup);

        // Should trigger because 800/1000 = 80%
        assert!(tracker.compaction_needed);
    }

    #[test]
    fn observe_event_tokens_without_any_limit_does_not_trigger_compaction() {
        // Lookup won't be called since no model info event arrives
        let lookup = |_: &str, _: &str| Some(1000);
        let mut tracker = TokenTracker::new();

        // Tokens arrive but no model info ever comes
        let ev_tokens = mk_message_part_step_finish(Some(mk_token_usage(10_000)));
        tracker.observe_event(&ev_tokens, lookup);

        // Should NOT trigger because context_limit is None
        assert!(!tracker.compaction_needed);
        assert_eq!(tracker.context_limit, None);
    }

    #[test]
    fn to_log_token_usage_preserves_values_without_saturation() {
        let mut tracker = TokenTracker::new();
        tracker.observe_tokens(&TokenUsage {
            total: Some(30),
            input: 10,
            output: 20,
            reasoning: 5,
            cache: None,
            extra: serde_json::Value::Null,
        });

        let (usage, saturated) = tracker.to_log_token_usage();
        let usage = usage.expect("usage should be present");
        assert!(!saturated);
        assert_eq!(usage.prompt, 10);
        assert_eq!(usage.completion, 20);
        assert_eq!(usage.total, 30);
        assert_eq!(usage.reasoning_tokens, Some(5));
    }

    #[test]
    fn to_log_token_usage_saturates_large_values() {
        let mut tracker = TokenTracker::new();
        tracker.observe_tokens(&TokenUsage {
            total: Some(u64::MAX),
            input: u64::MAX,
            output: u64::MAX,
            reasoning: u64::MAX,
            cache: None,
            extra: serde_json::Value::Null,
        });

        let (usage, saturated) = tracker.to_log_token_usage();
        let usage = usage.expect("usage should be present");
        assert!(saturated);
        assert_eq!(usage.prompt, u32::MAX);
        assert_eq!(usage.completion, u32::MAX);
        assert_eq!(usage.total, u32::MAX);
        assert_eq!(usage.reasoning_tokens, Some(u32::MAX));
    }
}
