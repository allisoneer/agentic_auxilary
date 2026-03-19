pub mod directory;
pub mod guards;
pub mod memory;
pub mod orchestration;
pub mod paths;

// Re-exports: only selectively export what the crate root needs
pub use directory::expand_directories_to_filemeta;
pub use guards::ensure_plan_template_group;
pub use guards::ensure_xml_has_group_marker;
pub use guards::maybe_inject_plan_structure_meta;
pub use memory::auto_inject_claude_memories;
pub use memory::injection_enabled_from_env;
pub use memory::memory_files_in_dir;
pub use orchestration::gpt5_reasoner_impl;
pub use paths::dedup_files_in_place;
pub use paths::is_ancestor;
pub use paths::normalize_paths_in_place;
pub use paths::precheck_files;
pub use paths::to_abs_string;
pub use paths::walk_up_to_boundary;
