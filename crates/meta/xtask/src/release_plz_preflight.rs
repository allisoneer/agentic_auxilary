use crate::policy::Policy;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use cargo_metadata::Metadata;
use cargo_metadata::MetadataCommand;
use reqwest::StatusCode;
use std::collections::BTreeSet;
use std::thread;
use std::time::Duration;
use std::time::Instant;

const CRATES_IO_API_BASE: &str = "https://crates.io/api/v1/crates";
const CRATES_IO_USER_AGENT: &str =
    "xtask-release-plz-preflight (+https://github.com/allisoneer/agentic_auxilary)";
const CRATES_IO_PACE_INTERVAL: Duration = Duration::from_secs(1);
const CRATES_IO_TIMEOUT: Duration = Duration::from_secs(10);

pub trait CratesIoClient {
    fn crate_exists(&self, crate_name: &str) -> Result<bool>;
}

trait RequestPacer {
    fn pace(&mut self);
}

struct FixedIntervalPacer {
    interval: Duration,
    last: Option<Instant>,
}

impl FixedIntervalPacer {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }
}

impl RequestPacer for FixedIntervalPacer {
    fn pace(&mut self) {
        if let Some(last) = self.last {
            let elapsed = last.elapsed();
            if let Some(remaining) = self.interval.checked_sub(elapsed) {
                thread::sleep(remaining);
            }
        }

        self.last = Some(Instant::now());
    }
}

struct HttpCratesIoClient {
    client: reqwest::blocking::Client,
    base_url: String,
}

impl HttpCratesIoClient {
    fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(CRATES_IO_USER_AGENT)
            .timeout(CRATES_IO_TIMEOUT)
            .build()
            .context("Failed to build crates.io HTTP client")?;
        Ok(Self {
            client,
            base_url: CRATES_IO_API_BASE.to_string(),
        })
    }
}

impl CratesIoClient for HttpCratesIoClient {
    fn crate_exists(&self, crate_name: &str) -> Result<bool> {
        let url = format!("{}/{crate_name}", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("Failed to query crates.io for crate `{crate_name}`"))?;

        match response.status() {
            StatusCode::OK => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => bail!(
                "crates.io lookup for `{crate_name}` returned unexpected HTTP status {status}"
            ),
        }
    }
}

fn publish_enabled(crate_name: &str, policy: &Policy) -> bool {
    policy
        .release_plz
        .overrides
        .get(crate_name)
        .and_then(|entry| entry.publish)
        .unwrap_or(policy.release_plz.publish_default)
}

fn workspace_package_names(metadata: &Metadata) -> Vec<String> {
    metadata
        .packages
        .iter()
        .filter(|pkg| metadata.workspace_members.contains(&pkg.id))
        .map(|pkg| pkg.name.clone())
        .collect()
}

fn publishable_crate_names<I>(crate_names: I, policy: &Policy) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    crate_names
        .into_iter()
        .filter(|crate_name| publish_enabled(crate_name, policy))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn missing_publishable_crates(
    client: &dyn CratesIoClient,
    crate_names: &[String],
    pacer: &mut dyn RequestPacer,
) -> Result<Vec<String>> {
    let mut missing = Vec::new();

    for crate_name in crate_names {
        pacer.pace();
        if !client.crate_exists(crate_name)? {
            missing.push(crate_name.clone());
        }
    }

    Ok(missing)
}

fn ensure_no_missing_crates(missing: &[String]) -> Result<()> {
    if missing.is_empty() {
        return Ok(());
    }

    bail!(
        "release-plz preflight failed:\n\
         The following crates are configured to publish to crates.io but were not found:\n\
         - {}\n\n\
         This usually means the CI token cannot publish new crates.\n\
         Remediation:\n\
         1) First-publish locally: `cargo publish -p <crate>` once per crate, OR\n\
         2) If the crate should not be published, set `release_plz.overrides.<crate>.publish = false` in tools/policy.toml and run `cargo run -p xtask -- sync`.",
        missing.join("\n- ")
    );
}

pub fn run() -> Result<()> {
    eprintln!("[release-plz-preflight] Loading workspace metadata...");
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to run `cargo metadata`")?;

    eprintln!("[release-plz-preflight] Loading policy from tools/policy.toml...");
    let policy = Policy::load()?;
    let client = HttpCratesIoClient::new()?;
    let crate_names = publishable_crate_names(workspace_package_names(&metadata), &policy);
    let mut pacer = FixedIntervalPacer::new(CRATES_IO_PACE_INTERVAL);
    let missing = missing_publishable_crates(&client, &crate_names, &mut pacer)?;
    ensure_no_missing_crates(&missing)?;

    eprintln!("[release-plz-preflight] All publishable crates exist on crates.io.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    struct FakeCratesIoClient {
        existing: BTreeSet<String>,
    }

    struct CountingPacer {
        count: usize,
    }

    struct NoopPacer;

    impl CratesIoClient for FakeCratesIoClient {
        fn crate_exists(&self, crate_name: &str) -> Result<bool> {
            Ok(self.existing.contains(crate_name))
        }
    }

    impl RequestPacer for CountingPacer {
        fn pace(&mut self) {
            self.count += 1;
        }
    }

    impl RequestPacer for NoopPacer {
        fn pace(&mut self) {}
    }

    fn parse_policy(toml: &str) -> Policy {
        toml::from_str(toml).expect("test policy should parse")
    }

    #[test]
    fn publishable_crates_follow_policy_overrides() {
        let policy = parse_policy(
            r#"
[enums]
role = ["app"]
family = ["tools"]

[integrations]

[paths]
enforce = false

[todos]
blocked_severities = [0]

[release_plz]
git_tag_format = "{{ name }}-v{{ version }}"
publish_default = true
git_tag_enable_default = false

[release_plz.overrides.not-published]
publish = false
"#,
        );

        let publishable = publishable_crate_names(
            vec![
                "published-app".to_string(),
                "not-published".to_string(),
                "published-lib".to_string(),
            ],
            &policy,
        );

        assert_eq!(
            publishable,
            vec!["published-app".to_string(), "published-lib".to_string()]
        );
    }

    #[test]
    fn missing_crates_fail_with_remediation_message() {
        let client = FakeCratesIoClient {
            existing: BTreeSet::from(["existing-crate".to_string()]),
        };
        let mut pacer = NoopPacer;
        let missing = missing_publishable_crates(
            &client,
            &["existing-crate".to_string(), "new-crate".to_string()],
            &mut pacer,
        )
        .expect("missing crate detection should succeed");

        let error = ensure_no_missing_crates(&missing)
            .expect_err("missing crates should hard-fail")
            .to_string();

        assert!(error.contains("release-plz preflight failed"));
        assert!(error.contains("new-crate"));
        assert!(error.contains("cargo publish -p <crate>"));
        assert!(error.contains("release_plz.overrides.<crate>.publish = false"));
    }

    #[test]
    fn existing_crates_pass() {
        let client = FakeCratesIoClient {
            existing: BTreeSet::from([
                "another-existing-crate".to_string(),
                "existing-crate".to_string(),
            ]),
        };
        let mut pacer = NoopPacer;
        let missing = missing_publishable_crates(
            &client,
            &[
                "existing-crate".to_string(),
                "another-existing-crate".to_string(),
            ],
            &mut pacer,
        )
        .expect("lookup should succeed");

        assert!(missing.is_empty());
        ensure_no_missing_crates(&missing).expect("no missing crates should pass");
    }

    #[test]
    fn missing_publishable_crates_invokes_pacing_once_per_lookup() {
        let client = FakeCratesIoClient {
            existing: BTreeSet::from(["existing-crate".to_string()]),
        };
        let mut pacer = CountingPacer { count: 0 };
        let missing = missing_publishable_crates(
            &client,
            &[
                "existing-crate".to_string(),
                "missing-crate".to_string(),
                "another-missing-crate".to_string(),
            ],
            &mut pacer,
        )
        .expect("lookup should succeed");

        assert_eq!(pacer.count, 3);
        assert_eq!(
            missing,
            vec![
                "missing-crate".to_string(),
                "another-missing-crate".to_string()
            ]
        );
    }
}
