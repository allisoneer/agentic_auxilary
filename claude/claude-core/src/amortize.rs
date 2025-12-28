//! Amortized string for smooth streaming display

const CHUNK_SIZE: usize = 5;

/// Amortized string that yields characters in chunks for smooth display
#[derive(Default, Debug)]
pub struct AmortizedString {
    text: String,
    tail: usize,
    done: bool,
}

impl AmortizedString {
    /// Update the full text content
    pub fn update(&mut self, text: String) {
        if !text.starts_with(&self.text) {
            // Text was replaced, not appended
            self.tail = 0;
        }
        self.text = text;
        self.done = false;
    }

    /// Get the current displayed portion
    pub fn current(&self) -> &str {
        &self.text[..self.tail]
    }

    /// Check if all text has been yielded
    pub fn is_complete(&self) -> bool {
        self.done && self.tail == self.text.len()
    }
}

impl Iterator for AmortizedString {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        self.tail = (self.tail + CHUNK_SIZE).min(self.text.len());

        // Ensure we're at a char boundary
        while self.tail < self.text.len() && !self.text.is_char_boundary(self.tail) {
            self.tail += 1;
        }

        self.done = self.tail >= self.text.len();
        Some(self.text[..self.tail].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_amortization() {
        let mut amort = AmortizedString::default();
        amort.update("Hello, World!".to_string());

        let chunks: Vec<_> = (&mut amort).collect();
        assert!(!chunks.is_empty());
        assert_eq!(chunks.last().unwrap(), "Hello, World!");
    }
}
