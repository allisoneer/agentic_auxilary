use crate::errors::ReasonerError;
use crate::errors::Result;
use tiktoken_rs::o200k_base;

pub const TOKEN_LIMIT: usize = 250_000;

pub fn count_tokens(text: &str) -> Result<usize> {
    let bpe = o200k_base().map_err(|e| ReasonerError::Template(format!("Tokenizer error: {e}")))?;
    Ok(bpe.encode_with_special_tokens(text).len())
}

pub fn enforce_limit(text: &str, max_input_tokens: Option<u32>) -> Result<()> {
    let limit = max_input_tokens.map_or(TOKEN_LIMIT, |n| n as usize);
    let n = count_tokens(text)?;
    if n > limit {
        return Err(ReasonerError::TokenLimit { current: n, limit });
    }
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::allow_attributes,
    reason = "incremental legacy lint mitigation for pre-existing tests"
)]
// TODO(3): clean up unwrap_used as part of broader gpt5_reasoner lint conformance pass.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_count_tokens_basic() {
        // Simple test with a known string
        let text = "Hello world";
        let count = count_tokens(text).unwrap();
        // We know this should be a small number
        assert!(count > 0);
        assert!(count < 10);
    }

    #[test]
    fn test_enforce_limit_under() {
        let text = "This is a short text that should be well under the limit";
        assert!(enforce_limit(text, None).is_ok());
    }

    #[test]
    fn test_enforce_limit_over() {
        // Create a very long string that will exceed 250k tokens
        // Each "word " is roughly 1-2 tokens, so 300k words should exceed limit
        let long_text = "word ".repeat(300_000);
        let result = enforce_limit(&long_text, None);
        assert!(result.is_err());
        match result.unwrap_err() {
            ReasonerError::TokenLimit { current, limit } => {
                assert!(current > limit);
                assert_eq!(limit, TOKEN_LIMIT);
            }
            _ => panic!("Expected TokenLimit error"),
        }
    }

    #[test]
    fn test_enforce_limit_uses_configured_max_input_tokens() {
        let text = "word ".repeat(32);
        let result = enforce_limit(&text, Some(1));
        assert!(result.is_err());
        match result.unwrap_err() {
            ReasonerError::TokenLimit { limit, .. } => assert_eq!(limit, 1),
            _ => panic!("Expected TokenLimit error"),
        }
    }
}
