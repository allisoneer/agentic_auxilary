//! Binary format metadata for instant-grep index generations.

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexMeta {
    pub format_version: u32,
    pub generation_version: u32,
    pub head_oid: String,
    pub repo_root: String,
    pub branch_key: String,
    pub doc_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupEntry {
    pub gram: u64,
    pub postings_offset_bytes: u64,
    pub postings_len_bytes: u32,
    pub doc_freq: u32,
}
