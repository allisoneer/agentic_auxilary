pub mod config;
pub mod directory;
pub mod guards;
pub mod memory;
pub mod orchestration;
pub mod paths;

// Re-exports: only selectively export what the crate root needs
pub use config::select_optimizer_model;
pub use directory::expand_directories_to_filemeta;
pub use guards::{
    ensure_plan_template_group, ensure_xml_has_group_marker, maybe_inject_plan_structure_meta,
};
pub use memory::{auto_inject_claude_memories, injection_enabled_from_env, memory_files_in_dir};
pub use orchestration::gpt5_reasoner_impl;
pub use paths::{
    dedup_files_in_place, is_ancestor, normalize_paths_in_place, precheck_files, to_abs_string,
    walk_up_to_boundary,
};
