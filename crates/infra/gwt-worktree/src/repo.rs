use crate::config::GwtConfig;
use crate::error::Error;
use crate::error::Result;
use git2::ErrorCode;
use git2::Repository;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

// Compatibility-critical sentinel written for upstream gwt interoperability.
// Do not edit without intentionally updating byte-for-byte regression coverage.
const README_SENTINEL: &str = "# Git Worktree Directory\n\nThis directory contains git worktrees managed by the gwt tool.\nEach subdirectory is a separate worktree for a branch.\n\nFor more information, see: https://github.com/General-Wisdom/gwt\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlRepo {
    pub git_dir_key: String,
    pub common_dir: PathBuf,
    pub worktree_base: PathBuf,
    pub main_workdir: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct ResolveControlRepoOptions<'a> {
    pub path: Option<&'a Path>,
    pub cwd: Option<&'a Path>,
    pub env_git_dir: Option<&'a str>,
    pub config: Option<&'a GwtConfig>,
}

impl ControlRepo {
    pub fn from_git_dir(git_dir_key: impl Into<String>) -> Result<Self> {
        let git_dir_key = normalize_git_dir_input(git_dir_key.into())?;
        let common_dir = PathBuf::from(&git_dir_key);
        let repo = Repository::open(&common_dir)?;
        let common_dir = repo.commondir().to_path_buf();
        let worktree_base = compute_gwt_base_from_parts(&git_dir_key, &common_dir);
        let main_workdir = main_workdir_from_repo(&repo);

        Ok(Self {
            git_dir_key,
            common_dir,
            worktree_base,
            main_workdir,
        })
    }

    pub fn resolve(options: &ResolveControlRepoOptions<'_>) -> Result<Self> {
        if let Some(path) = options.path.or(options.cwd) {
            match resolve_from_discovery(path) {
                Ok(repo) => return Ok(repo),
                Err(Error::Git(error)) if error.code() == ErrorCode::NotFound => {}
                Err(err) => return Err(err),
            }
        }

        if let Some(env_git_dir) = options.env_git_dir {
            return Self::from_git_dir(env_git_dir);
        }

        if let Some(config) = options.config
            && let Some(default_repo) = &config.default_repo
        {
            return Self::from_git_dir(default_repo.clone());
        }

        Err(Error::ControlRepoNotFound)
    }

    pub fn ensure_worktree_base(&self) -> Result<()> {
        ensure_worktree_base_dir(&self.worktree_base)
    }
}

pub fn compute_gwt_base(git_dir: &str) -> PathBuf {
    let resolved = Path::new(git_dir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(git_dir));
    compute_gwt_base_from_parts(git_dir, &resolved)
}

pub fn compute_gwt_base_from_parts(git_dir: &str, resolved_git_dir: &Path) -> PathBuf {
    if resolved_git_dir
        .file_name()
        .is_some_and(|name| name == ".git")
        && resolved_git_dir.is_dir()
    {
        return PathBuf::from(format!(
            "{}.gwt",
            resolved_git_dir
                .parent()
                .unwrap_or(resolved_git_dir)
                .display()
        ));
    }

    let trimmed = git_dir.trim_end_matches('/');
    if has_exact_git_suffix(trimmed) {
        return PathBuf::from(format!("{}.gwt", trimmed.trim_end_matches(".git")));
    }

    PathBuf::from(format!("{trimmed}.gwt"))
}

pub fn main_worktree_path(git_dir: &str) -> Option<PathBuf> {
    let resolved = Path::new(git_dir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(git_dir));
    if resolved.file_name().is_some_and(|name| name == ".git") && resolved.is_dir() {
        resolved.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

pub fn ensure_worktree_base_dir(base_dir: &Path) -> Result<()> {
    if base_dir.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(base_dir)?;
    std::fs::write(base_dir.join("README.md"), README_SENTINEL)?;
    Ok(())
}

pub fn readme_sentinel_contents() -> &'static str {
    README_SENTINEL
}

fn resolve_from_discovery(path: &Path) -> Result<ControlRepo> {
    let repo = Repository::discover(path)?;
    let common_dir = repo.commondir().to_path_buf();
    let git_dir_key = normalize_git_dir_key(&common_dir);
    let worktree_base = compute_gwt_base_from_parts(&git_dir_key, &common_dir);
    let main_workdir = main_workdir_from_repo(&repo);

    Ok(ControlRepo {
        git_dir_key,
        common_dir,
        worktree_base,
        main_workdir,
    })
}

fn main_workdir_from_repo(repo: &Repository) -> Option<PathBuf> {
    if repo.is_bare() {
        None
    } else {
        repo.workdir().map(Path::to_path_buf)
    }
}

fn normalize_git_dir_input(git_dir: String) -> Result<String> {
    let input = PathBuf::from(&git_dir);

    if input.is_dir() {
        let dot_git = input.join(".git");
        if dot_git.is_dir() {
            return Ok(dot_git.to_string_lossy().into_owned());
        }
        if dot_git.is_file() {
            return resolve_gitdir_pointer(&dot_git);
        }
    }

    if input.is_file() && input.file_name().is_some_and(|name| name == ".git") {
        return resolve_gitdir_pointer(&input);
    }

    Ok(git_dir)
}

fn resolve_gitdir_pointer(dot_git_file: &Path) -> Result<String> {
    let contents = std::fs::read_to_string(dot_git_file)?;
    let Some(line) = contents.lines().next() else {
        return Ok(dot_git_file.to_string_lossy().into_owned());
    };
    let Some(pointer) = line.strip_prefix("gitdir:") else {
        return Ok(dot_git_file.to_string_lossy().into_owned());
    };

    let raw_path = PathBuf::from(pointer.trim());
    let resolved = if raw_path.is_absolute() {
        raw_path
    } else {
        dot_git_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(raw_path)
    };

    let resolved = resolved
        .canonicalize()
        .unwrap_or_else(|_| normalize_path(resolved.as_path()));

    Ok(resolved.to_string_lossy().into_owned())
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    normalized
}

fn normalize_git_dir_key(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if raw.len() > 1 {
        raw.trim_end_matches('/').to_owned()
    } else {
        raw.into_owned()
    }
}

fn has_exact_git_suffix(git_dir: &str) -> bool {
    git_dir.as_bytes().ends_with(b".git")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const EXPECTED_README_SENTINEL: &str = "# Git Worktree Directory\n\nThis directory contains git worktrees managed by the gwt tool.\nEach subdirectory is a separate worktree for a branch.\n\nFor more information, see: https://github.com/General-Wisdom/gwt\n";

    #[test]
    fn computes_base_for_normal_repo_git_dir() {
        let temp = TempDir::new().unwrap();
        let temp_root = canonical_temp_root(&temp);
        let repo_root = temp_root.join("repo");
        std::fs::create_dir_all(&repo_root).unwrap();
        let git_dir = repo_root.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();

        let base = compute_gwt_base(git_dir.to_str().unwrap());

        assert_eq!(base, repo_root.with_extension("gwt"));
    }

    #[test]
    fn computes_base_for_bare_suffix_repo() {
        let base = compute_gwt_base("/tmp/example.git");
        assert_eq!(base, PathBuf::from("/tmp/example.gwt"));
    }

    #[test]
    fn computes_base_for_bare_suffix_repo_with_trailing_slash() {
        let base = compute_gwt_base("/tmp/example.git/");
        assert_eq!(base, PathBuf::from("/tmp/example.gwt"));
    }

    #[test]
    fn computes_base_for_fallback_path() {
        let base = compute_gwt_base("/tmp/example/");
        assert_eq!(base, PathBuf::from("/tmp/example.gwt"));
    }

    #[test]
    fn main_worktree_path_is_none_for_bare_style_repo() {
        assert_eq!(main_worktree_path("/tmp/example.git"), None);
    }

    #[test]
    fn creates_readme_only_on_first_base_creation() {
        let temp = TempDir::new().unwrap();
        let temp_root = canonical_temp_root(&temp);
        let base = temp_root.join("repo.gwt");

        ensure_worktree_base_dir(&base).unwrap();
        let readme = base.join("README.md");
        let first_contents = std::fs::read_to_string(&readme).unwrap();
        assert_eq!(first_contents, readme_sentinel_contents());

        std::fs::write(&readme, "custom").unwrap();
        ensure_worktree_base_dir(&base).unwrap();

        assert_eq!(std::fs::read_to_string(&readme).unwrap(), "custom");
    }

    #[test]
    fn readme_sentinel_uses_canonical_gwt_url() {
        assert_eq!(readme_sentinel_contents(), EXPECTED_README_SENTINEL);
    }

    #[test]
    fn resolves_bare_repo_without_main_workdir() {
        let temp = TempDir::new().unwrap();
        let temp_root = canonical_temp_root(&temp);
        let bare_path = temp_root.join("example.git");
        Repository::init_bare(&bare_path).unwrap();

        let repo = ControlRepo::from_git_dir(bare_path.to_str().unwrap()).unwrap();

        assert_eq!(repo.main_workdir, None);
        assert_eq!(repo.worktree_base, temp_root.join("example.gwt"));
    }

    #[test]
    fn normalizes_repo_root_inputs_to_dot_git() {
        let temp = TempDir::new().unwrap();
        let temp_root = canonical_temp_root(&temp);
        let repo_root = temp_root.join("repo");
        Repository::init(&repo_root).unwrap();

        let repo = ControlRepo::from_git_dir(repo_root.to_str().unwrap()).unwrap();

        assert_eq!(repo.git_dir_key, repo_root.join(".git").to_string_lossy());
    }

    #[test]
    fn resolves_from_explicit_path_before_env_and_config() {
        let temp = TempDir::new().unwrap();
        let temp_root = canonical_temp_root(&temp);
        let path_repo = temp_root.join("path-repo");
        let env_repo = temp_root.join("env-repo");
        let cfg_repo = temp_root.join("cfg-repo");
        Repository::init(&path_repo).unwrap();
        Repository::init(&env_repo).unwrap();
        Repository::init(&cfg_repo).unwrap();

        let config = GwtConfig {
            default_repo: Some(cfg_repo.join(".git").to_string_lossy().into_owned()),
            ..GwtConfig::default()
        };

        let resolved = ControlRepo::resolve(&ResolveControlRepoOptions {
            path: Some(path_repo.as_path()),
            cwd: None,
            env_git_dir: Some(env_repo.join(".git").to_str().unwrap()),
            config: Some(&config),
        })
        .unwrap();

        assert_eq!(
            resolved.git_dir_key,
            path_repo.join(".git").to_string_lossy()
        );
    }

    #[test]
    fn resolves_from_env_before_config_when_discovery_missing() {
        let temp = TempDir::new().unwrap();
        let temp_root = canonical_temp_root(&temp);
        let env_repo = temp_root.join("env-repo");
        let cfg_repo = temp_root.join("cfg-repo");
        Repository::init(&env_repo).unwrap();
        Repository::init(&cfg_repo).unwrap();

        let config = GwtConfig {
            default_repo: Some(cfg_repo.join(".git").to_string_lossy().into_owned()),
            ..GwtConfig::default()
        };

        let resolved = ControlRepo::resolve(&ResolveControlRepoOptions {
            path: Some(temp_root.as_path()),
            cwd: None,
            env_git_dir: Some(env_repo.join(".git").to_str().unwrap()),
            config: Some(&config),
        })
        .unwrap();

        assert_eq!(
            resolved.git_dir_key,
            env_repo.join(".git").to_string_lossy()
        );
    }

    #[test]
    fn resolves_worktree_gitfile_inputs_to_private_gitdir() {
        let temp = TempDir::new().unwrap();
        let temp_root = canonical_temp_root(&temp);
        let main_repo = temp_root.join("main");
        let worktree_path = temp_root.join("main.gwt").join("feature");
        let repo = Repository::init(&main_repo).unwrap();
        commit_initial(&repo);
        std::fs::create_dir_all(worktree_path.parent().unwrap()).unwrap();
        repo.worktree("feature", &worktree_path, None).unwrap();

        let resolved = ControlRepo::from_git_dir(worktree_path.to_str().unwrap()).unwrap();

        assert!(resolved.git_dir_key.contains(".git/worktrees/feature"));
        assert_eq!(resolved.common_dir, main_repo.join(".git"));
    }

    #[test]
    fn resolves_gitdir_pointer_to_normalized_path() {
        let temp = TempDir::new().unwrap();
        let temp_root = canonical_temp_root(&temp);
        let repo_root = temp_root.join("repo");
        let dot_git = repo_root.join(".git");
        let private_gitdir = temp_root.join("private").join("worktrees").join("feature");
        std::fs::create_dir_all(&repo_root).unwrap();
        std::fs::create_dir_all(&private_gitdir).unwrap();
        std::fs::write(
            &dot_git,
            "gitdir: ../private/./worktrees/feature/../feature\n",
        )
        .unwrap();

        let resolved = resolve_gitdir_pointer(&dot_git).unwrap();

        assert_eq!(
            PathBuf::from(resolved),
            private_gitdir.canonicalize().unwrap()
        );
    }

    fn canonical_temp_root(temp: &TempDir) -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            temp.path()
                .canonicalize()
                .expect("canonicalize TempDir path on macOS")
        }
        #[cfg(not(target_os = "macos"))]
        {
            temp.path().to_path_buf()
        }
    }

    fn commit_initial(repo: &Repository) {
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
    }
}
