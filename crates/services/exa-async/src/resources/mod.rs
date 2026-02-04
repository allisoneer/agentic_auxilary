//! API resource implementations for the Exa client

/// Answer API resource
pub mod answer;
/// Contents API resource
pub mod contents;
/// Find-similar API resource
pub mod find_similar;
/// Search API resource
pub mod search;

pub use answer::Answer;
pub use contents::Contents;
pub use find_similar::FindSimilar;
pub use search::Search;
