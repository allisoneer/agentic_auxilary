//! On-disk instant-grep index support.

pub mod builder;
pub mod format;
pub mod reader;
pub mod storage;

/// Marker constant for the first instant-grep index generation.
pub const GENERATION_VERSION: u32 = 1;
