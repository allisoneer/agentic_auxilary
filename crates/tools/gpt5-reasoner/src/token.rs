use crate::errors::{ReasonerError, Result};
use tiktoken_rs::o200k_base;

pub const TOKEN_LIMIT: usize = 250_000;

pub fn count_tokens(text: &str) -> Result<usize> {
    let bpe = o200k_base().map_err(|e| ReasonerError::Template(format!("Tokenizer error: {e}")))?;
    Ok(bpe.encode_with_special_tokens(text).len())
}

pub fn enforce_limit(text: &str) -> Result<()> {
    let n = count_tokens(text)?;
    if n > TOKEN_LIMIT {
        return Err(ReasonerError::TokenLimit {
            current: n,
            limit: TOKEN_LIMIT,
        });
    }
    Ok(())
}

#[cfg(test)]
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
        assert!(enforce_limit(text).is_ok());
    }

    #[test]
    fn test_enforce_limit_over() {
        // Create a very long string that will exceed 250k tokens
        // Each "word " is roughly 1-2 tokens, so 300k words should exceed limit
        let long_text = "word ".repeat(300_000);
        let result = enforce_limit(&long_text);
        assert!(result.is_err());
        match result.unwrap_err() {
            ReasonerError::TokenLimit { current, limit } => {
                assert!(current > limit);
                assert_eq!(limit, TOKEN_LIMIT);
            }
            _ => panic!("Expected TokenLimit error"),
        }
    }
}
