use gwt_worktree::command::CommandSpec;
use gwt_worktree::config::GwtConfig;
use gwt_worktree::repo::ControlRepo;
use gwt_worktree::types::BranchName;
use gwt_worktree::worktree::WorktreeInfo;
use std::path::PathBuf;

#[test]
fn root_exports_compile() {
    let _ = gwt_worktree::Result::<()>::Ok(());
    let _ = CommandSpec::from("thoughts init");
    let _ = GwtConfig::default();
    let _ = ControlRepo {
        git_dir_key: String::new(),
        common_dir: PathBuf::new(),
        worktree_base: PathBuf::new(),
        main_workdir: None,
    };
    let _ = BranchName::new("feature/foo");
    let _ = WorktreeInfo {
        path: PathBuf::new(),
        head: None,
        branch: None,
        is_main: false,
        locked: false,
        prunable: false,
        detached: false,
    };
}
