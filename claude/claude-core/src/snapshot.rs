//! Snapshot accumulator for streaming responses

use crate::CoreError;
use anthropic_async::streaming::{Accumulator, Event};

/// Accumulator that converts SSE events to text snapshots
pub struct SnapshotAccumulator {
    acc: Accumulator,
}

impl SnapshotAccumulator {
    /// Create a new snapshot accumulator
    pub fn new() -> Self {
        Self {
            acc: Accumulator::new(),
        }
    }

    /// Apply an event and return (is_complete, current_text_snapshot)
    pub fn apply(&mut self, event: &Event) -> Result<(bool, String), CoreError> {
        let response = self
            .acc
            .apply(event)
            .map_err(|e| CoreError::Api(e.to_string()))?;

        let current_text = self.acc.current_text();
        let is_complete = response.is_some();

        Ok((is_complete, current_text))
    }

    /// Get the current accumulated text
    pub fn current_text(&self) -> String {
        self.acc.current_text()
    }
}

impl Default for SnapshotAccumulator {
    fn default() -> Self {
        Self::new()
    }
}
