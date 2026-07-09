use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhCheck {
    pub name: String,
    pub state: String,
    pub conclusion: Option<String>,
    pub details_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequiredCiPoll {
    Waiting {
        pending: Vec<GhCheck>,
        fingerprint: String,
    },
    Passed {
        fingerprint: String,
    },
    Failed {
        failing: Vec<GhCheck>,
        fingerprint: String,
    },
}

#[derive(Debug, Deserialize)]
struct GhPrCheckRow {
    name: String,
    state: String,
    #[serde(default)]
    link: Option<String>,
}

pub struct RequiredCiClient {
    gh_binary: &'static str,
}

impl RequiredCiClient {
    pub fn new() -> Self {
        Self { gh_binary: "gh" }
    }

    pub fn poll_once(&self, pr_number: u64) -> Result<RequiredCiPoll> {
        let checks = list_pr_checks(self.gh_binary, pr_number)?;
        Ok(interpret_required_ci(&checks))
    }
}

pub fn interpret_required_ci(all_checks: &[GhCheck]) -> RequiredCiPoll {
    let required_checks = required_checks(all_checks);
    let fingerprint = required_ci_fingerprint(&required_checks);

    let pending: Vec<_> = required_checks
        .iter()
        .filter(|check| is_pending_state(&check.state, check.conclusion.as_deref()))
        .cloned()
        .collect();
    if !pending.is_empty() {
        return RequiredCiPoll::Waiting {
            pending,
            fingerprint,
        };
    }

    let failing: Vec<_> = required_checks
        .iter()
        .filter(|check| is_failed_state(&check.state, check.conclusion.as_deref()))
        .cloned()
        .collect();
    if !failing.is_empty() {
        return RequiredCiPoll::Failed {
            failing,
            fingerprint,
        };
    }

    RequiredCiPoll::Passed { fingerprint }
}

pub fn required_ci_fingerprint(required_checks: &[GhCheck]) -> String {
    let mut entries: Vec<_> = required_checks
        .iter()
        .map(|check| {
            format!(
                "{}|{}|{}|{}",
                check.name,
                check.state,
                check.conclusion.as_deref().unwrap_or(""),
                check.details_url.as_deref().unwrap_or("")
            )
        })
        .collect();
    entries.sort();
    entries.join("\n")
}

fn list_pr_checks(gh_binary: &str, pr_number: u64) -> Result<Vec<GhCheck>> {
    // TODO(2): If slow or hung `gh pr checks` invocations prove to block outer-dag
    // responsiveness, move this polling off the async executor and/or add a subprocess
    // timeout (for example via `spawn_blocking` or `tokio::process::Command` with timeout).
    let output = Command::new(gh_binary)
        .args([
            "pr",
            "checks",
            &pr_number.to_string(),
            "--json",
            "name,state,link",
        ])
        .output()
        .with_context(|| format!("failed to run gh pr checks for PR #{pr_number}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "gh pr checks failed for PR #{pr_number}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let rows: Vec<GhPrCheckRow> = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("failed to parse gh pr checks JSON for PR #{pr_number}"))?;

    Ok(rows.into_iter().map(Into::into).collect())
}

fn required_checks(all_checks: &[GhCheck]) -> Vec<GhCheck> {
    all_checks
        .iter()
        .filter(|check| !is_coderabbit_check(check))
        .filter(|check| !is_ignored_state(&check.state, check.conclusion.as_deref()))
        .cloned()
        .collect()
}

fn is_coderabbit_check(check: &GhCheck) -> bool {
    let name = check.name.to_ascii_lowercase();
    let url = check
        .details_url
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    name.contains("coderabbit") || url.contains("coderabbit")
}

fn is_ignored_state(state: &str, conclusion: Option<&str>) -> bool {
    matches!(
        normalized_state(state, conclusion),
        Some("skipped" | "neutral")
    )
}

fn is_pending_state(state: &str, conclusion: Option<&str>) -> bool {
    matches!(
        normalized_state(state, conclusion),
        Some("queued" | "in_progress" | "pending")
    )
}

fn is_failed_state(state: &str, conclusion: Option<&str>) -> bool {
    matches!(
        normalized_state(state, conclusion),
        Some("failure" | "cancelled" | "timed_out" | "action_required")
    )
}

fn normalized_state<'a>(state: &'a str, conclusion: Option<&'a str>) -> Option<&'static str> {
    let normalized = conclusion.unwrap_or(state).trim().to_ascii_lowercase();
    match normalized.as_str() {
        "queued" | "requested" | "waiting" | "pending" => Some("pending"),
        "in_progress" => Some("in_progress"),
        "success" | "pass" | "passed" => Some("success"),
        "failure" | "fail" | "failed" | "error" => Some("failure"),
        "cancelled" | "canceled" => Some("cancelled"),
        "timed_out" | "timedout" | "startup_failure" => Some("timed_out"),
        "action_required" | "action required" => Some("action_required"),
        "skipped" | "skip" => Some("skipped"),
        "neutral" => Some("neutral"),
        "completed" => match conclusion
            .map(str::trim)
            .filter(|conclusion| !conclusion.is_empty())
        {
            Some(conclusion) => normalized_state(conclusion, None),
            None => Some("success"),
        },
        _ => None,
    }
}

impl From<GhPrCheckRow> for GhCheck {
    fn from(value: GhPrCheckRow) -> Self {
        Self {
            name: value.name,
            state: value.state,
            conclusion: None,
            details_url: value.link,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &str, state: &str, conclusion: Option<&str>) -> GhCheck {
        GhCheck {
            name: name.to_string(),
            state: state.to_string(),
            conclusion: conclusion.map(str::to_string),
            details_url: Some(format!("https://example.invalid/{name}")),
        }
    }

    #[test]
    fn pending_required_check_returns_waiting() {
        let poll = interpret_required_ci(&[check("test", "queued", None)]);
        assert!(matches!(poll, RequiredCiPoll::Waiting { pending, .. } if pending.len() == 1));
    }

    #[test]
    fn failed_required_conclusions_return_failed() {
        for state in ["failure", "cancelled", "timed_out", "action_required"] {
            let poll = interpret_required_ci(&[check("test", "completed", Some(state))]);
            assert!(matches!(poll, RequiredCiPoll::Failed { failing, .. } if failing.len() == 1));
        }
    }

    #[test]
    fn all_success_required_checks_return_passed() {
        let poll = interpret_required_ci(&[
            check("fmt", "success", None),
            check("unit", "completed", Some("success")),
        ]);
        assert!(matches!(poll, RequiredCiPoll::Passed { .. }));
    }

    #[test]
    fn skipped_and_neutral_checks_are_ignored() {
        let poll = interpret_required_ci(&[
            check("lint", "skipped", None),
            check("docs", "neutral", None),
        ]);
        assert!(matches!(poll, RequiredCiPoll::Passed { fingerprint } if fingerprint.is_empty()));
    }

    #[test]
    fn coderabbit_checks_are_excluded_from_required_ci() {
        let poll = interpret_required_ci(&[
            check("CodeRabbit Review", "failure", None),
            check("unit", "success", None),
        ]);
        assert!(matches!(poll, RequiredCiPoll::Passed { .. }));
    }

    #[test]
    fn fingerprint_is_stable_across_input_order() {
        let a = vec![check("b", "success", None), check("a", "failure", None)];
        let b = vec![check("a", "failure", None), check("b", "success", None)];
        assert_eq!(required_ci_fingerprint(&a), required_ci_fingerprint(&b));
    }
}
