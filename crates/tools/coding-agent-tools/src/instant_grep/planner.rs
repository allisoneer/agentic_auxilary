//! Query planning for instant-grep.
//!
//! MVP note: v1 uses a conservative required-literal planner rather than the
//! blog's full ideal covering planner across arbitrary regex structure. That
//! means more scan fallbacks and potentially broader candidate sets than the
//! ideal design, but it preserves correctness and the sparse-gram storage model.

use crate::instant_grep::grams::GramKey;
use crate::instant_grep::grams::all_grams;
use crate::instant_grep::grams::gram_weight;
use crate::instant_grep::index::reader::InstantGrepIndex;
use std::collections::BTreeSet;

const MIN_LITERAL_LEN: usize = 3;
const MAX_SELECTED_GRAMS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub literals: Vec<String>,
    pub grams: Vec<GramKey>,
    pub candidate_doc_ids: Vec<u32>,
}

pub fn extract_required_literals(pattern: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut current = String::new();
    let mut chars = pattern.chars();
    let mut prev_was_gap = false;

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                if next.is_ascii_alphanumeric() {
                    return vec![];
                }
                current.push(next);
                prev_was_gap = false;
            } else {
                return vec![];
            }
            continue;
        }

        match ch {
            '.' => {
                if current.len() >= MIN_LITERAL_LEN {
                    literals.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                prev_was_gap = true;
            }
            '^' | '$' => {
                if current.len() >= MIN_LITERAL_LEN {
                    literals.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                prev_was_gap = false;
            }
            '*' | '+' | '?' => {
                if !prev_was_gap {
                    return vec![];
                }
                prev_was_gap = true;
            }
            '{' => {
                if !prev_was_gap {
                    return vec![];
                }
                let mut saw_close = false;
                for next in chars.by_ref() {
                    if next == '}' {
                        saw_close = true;
                        break;
                    }
                }
                if !saw_close {
                    return vec![];
                }
                prev_was_gap = true;
            }
            '|' | '[' | ']' | '(' | ')' => return vec![],
            _ => {
                current.push(ch);
                prev_was_gap = false;
            }
        }
    }

    if current.len() >= MIN_LITERAL_LEN {
        literals.push(current);
    }

    literals
}

pub fn plan_query(index: &InstantGrepIndex, pattern: &str) -> anyhow::Result<Option<Plan>> {
    let literals = extract_required_literals(pattern);
    if literals.is_empty() {
        return Ok(None);
    }

    let mut gram_candidates = Vec::new();
    for literal in &literals {
        let literal_grams: BTreeSet<_> = all_grams(literal.as_bytes()).collect();
        for gram in literal_grams {
            if let Some(postings) = index.postings(gram) {
                gram_candidates.push((gram, postings, gram_weight(gram)));
            }
        }
    }

    if gram_candidates.is_empty() {
        return Ok(None);
    }

    gram_candidates.sort_by(|a, b| {
        a.1.len()
            .cmp(&b.1.len())
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| a.0.cmp(&b.0))
    });

    if gram_candidates[0].1.len() * 5 >= index.meta.doc_count as usize * 3 {
        return Ok(None);
    }

    let selected: Vec<_> = gram_candidates
        .into_iter()
        .take(MAX_SELECTED_GRAMS)
        .collect();

    let mut candidates: BTreeSet<u32> = selected[0].1.iter().copied().collect();
    for (_, postings, _) in selected.iter().skip(1) {
        let next: BTreeSet<u32> = postings.iter().copied().collect();
        candidates = candidates.intersection(&next).copied().collect();
        if candidates.is_empty() {
            break;
        }
    }

    Ok(Some(Plan {
        literals,
        grams: selected.into_iter().map(|(gram, _, _)| gram).collect(),
        candidate_doc_ids: candidates.into_iter().collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_literal_runs_between_regex_metacharacters() {
        assert_eq!(
            extract_required_literals("foo.*bar"),
            vec!["foo".to_string(), "bar".to_string()]
        );
    }

    #[test]
    fn ignores_short_literal_runs() {
        assert!(extract_required_literals("a.*bc").is_empty());
    }

    #[test]
    fn escaped_characters_extend_literal_run() {
        assert_eq!(
            extract_required_literals(r"foo\.bar"),
            vec!["foo.bar".to_string()]
        );
    }

    #[test]
    fn alternation_forces_fallback() {
        assert!(extract_required_literals("foo|bar").is_empty());
    }

    #[test]
    fn quantifying_literal_forces_fallback() {
        assert!(extract_required_literals("colou?r").is_empty());
        assert!(extract_required_literals("ab+c").is_empty());
    }

    #[test]
    fn quantifying_gap_is_allowed() {
        assert_eq!(
            extract_required_literals("foo.*bar"),
            vec!["foo".to_string(), "bar".to_string()]
        );
    }
}
