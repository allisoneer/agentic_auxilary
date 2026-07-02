use crate::PLAN_STRUCTURE_FILENAME;
use crate::engine::paths;
use crate::errors::ReasonerError;
use crate::errors::Result;
use crate::optimizer;
use crate::optimizer::parser::FileGrouping;
use crate::optimizer::prompts;
use crate::types::FileMeta;
use crate::types::PromptType;
use std::collections::HashSet;

pub const MAX_UNIQUE_FILES: usize = 500;
pub const MAX_FS_BYTES: u64 = 25 * 1024 * 1024;
pub const MAX_OPTIMIZER_PROMPT_TOKENS_EST: usize = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregatePreflightStats {
    pub unique_files: usize,
    pub fs_bytes: u64,
    pub optimizer_prompt_tokens_est: usize,
}

fn file_fs_bytes(file: &FileMeta) -> Result<u64> {
    if file.filename == PLAN_STRUCTURE_FILENAME {
        return Ok(prompts::PLAN_STRUCTURE_TEMPLATE.len() as u64);
    }

    match std::fs::metadata(&file.filename) {
        Ok(metadata) => Ok(metadata.len()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err(ReasonerError::MissingFile(file.filename.clone().into()))
        }
        Err(err) => Err(err.into()),
    }
}

pub fn aggregate_corpus_preflight(
    prompt_type: &PromptType,
    prompt: &str,
    files: &[FileMeta],
) -> Result<AggregatePreflightStats> {
    let unique_files = files.len();
    if unique_files > MAX_UNIQUE_FILES {
        return Err(ReasonerError::CorpusFileLimit {
            current: unique_files,
            limit: MAX_UNIQUE_FILES,
        });
    }

    let fs_bytes = files.iter().try_fold(0_u64, |acc, file| {
        file_fs_bytes(file).map(|len| acc.saturating_add(len))
    })?;
    if fs_bytes > MAX_FS_BYTES {
        return Err(ReasonerError::CorpusByteLimit {
            current: fs_bytes,
            limit: MAX_FS_BYTES,
        });
    }

    let optimizer_prompt_tokens_est = crate::token::count_tokens(prompts::SYSTEM_OPTIMIZER)?
        + crate::token::count_tokens(&optimizer::build_user_prompt(prompt_type, prompt, files))?;
    if optimizer_prompt_tokens_est > MAX_OPTIMIZER_PROMPT_TOKENS_EST {
        return Err(ReasonerError::CorpusOptimizerPromptTokenEstimateLimit {
            current: optimizer_prompt_tokens_est,
            limit: MAX_OPTIMIZER_PROMPT_TOKENS_EST,
        });
    }

    Ok(AggregatePreflightStats {
        unique_files,
        fs_bytes,
        optimizer_prompt_tokens_est,
    })
}

pub fn selected_file_subset_preflight<S: std::hash::BuildHasher>(
    allowed: &HashSet<String, S>,
    groups: &FileGrouping,
) -> Result<()> {
    let mut seen = HashSet::new();
    let mut unknown = Vec::new();

    for path in groups
        .file_groups
        .iter()
        .flat_map(|group| group.files.iter().cloned())
        .filter(|path| seen.insert(path.clone()))
    {
        if path == PLAN_STRUCTURE_FILENAME {
            continue;
        }

        let normalized = paths::to_abs_string(&path);
        if !allowed.contains(&normalized) {
            unknown.push(path);
        }
    }

    if unknown.is_empty() {
        Ok(())
    } else {
        Err(ReasonerError::OptimizerSelectedUnknownFiles(unknown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimizer::parser::FileGroup;
    use crate::test_support::DirGuard;
    use tempfile::TempDir;

    #[test]
    fn aggregate_preflight_rejects_file_count_limit() {
        let files = (0..=MAX_UNIQUE_FILES)
            .map(|idx| FileMeta {
                filename: format!("file-{idx}.rs"),
                description: "desc".into(),
            })
            .collect::<Vec<_>>();

        let err = aggregate_corpus_preflight(&PromptType::Reasoning, "prompt", &files).unwrap_err();
        assert!(matches!(
            err,
            ReasonerError::CorpusFileLimit {
                current,
                limit
            } if current == MAX_UNIQUE_FILES + 1 && limit == MAX_UNIQUE_FILES
        ));
    }

    #[test]
    fn aggregate_preflight_rejects_byte_limit() {
        let td = TempDir::new().unwrap();
        let file = td.path().join("large.txt");
        std::fs::write(&file, "x").unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_len(MAX_FS_BYTES + 1)
            .unwrap();

        let files = vec![FileMeta {
            filename: file.to_string_lossy().to_string(),
            description: "large".into(),
        }];

        let err = aggregate_corpus_preflight(&PromptType::Reasoning, "prompt", &files).unwrap_err();
        assert!(matches!(
            err,
            ReasonerError::CorpusByteLimit {
                current,
                limit: MAX_FS_BYTES
            } if current > MAX_FS_BYTES
        ));
    }

    #[test]
    #[serial_test::serial(env)]
    fn aggregate_preflight_rejects_optimizer_prompt_token_estimate_limit() {
        let td = TempDir::new().unwrap();
        let _dir = DirGuard::set(td.path());
        let file = td.path().join("tiny.txt");
        std::fs::write(&file, "tiny").unwrap();

        let files = vec![FileMeta {
            filename: file.to_string_lossy().to_string(),
            description: "word ".repeat(MAX_OPTIMIZER_PROMPT_TOKENS_EST),
        }];

        let err = aggregate_corpus_preflight(&PromptType::Reasoning, "prompt", &files).unwrap_err();
        assert!(matches!(
            err,
            ReasonerError::CorpusOptimizerPromptTokenEstimateLimit {
                current,
                limit: MAX_OPTIMIZER_PROMPT_TOKENS_EST
            } if current > MAX_OPTIMIZER_PROMPT_TOKENS_EST
        ));
    }

    #[test]
    fn selected_file_subset_preflight_rejects_unknown_paths() {
        let allowed = HashSet::from(["/tmp/known.rs".to_string()]);
        let groups = FileGrouping {
            file_groups: vec![FileGroup {
                name: "code".into(),
                purpose: None,
                critical: None,
                files: vec!["/tmp/missing.rs".into()],
            }],
        };

        let err = selected_file_subset_preflight(&allowed, &groups).unwrap_err();
        assert!(matches!(
            err,
            ReasonerError::OptimizerSelectedUnknownFiles(paths) if paths == vec!["/tmp/missing.rs"]
        ));
    }

    #[test]
    fn selected_file_subset_preflight_allows_plan_structure() {
        let allowed = HashSet::new();
        let groups = FileGrouping {
            file_groups: vec![FileGroup {
                name: "plan_template".into(),
                purpose: None,
                critical: None,
                files: vec![PLAN_STRUCTURE_FILENAME.into()],
            }],
        };

        selected_file_subset_preflight(&allowed, &groups).unwrap();
    }

    #[test]
    #[serial_test::serial(env)]
    fn selected_file_subset_preflight_normalizes_relative_paths() {
        let td = TempDir::new().unwrap();
        let _dir = DirGuard::set(td.path());
        let file = td.path().join("src/lib.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "fn main() {}\n").unwrap();

        let allowed = HashSet::from([paths::to_abs_string(&file.to_string_lossy())]);
        let groups = FileGrouping {
            file_groups: vec![FileGroup {
                name: "code".into(),
                purpose: None,
                critical: None,
                files: vec!["src/lib.rs".into()],
            }],
        };

        selected_file_subset_preflight(&allowed, &groups).unwrap();
    }
}
