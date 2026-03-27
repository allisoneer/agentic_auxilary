use anyhow::Context;
use anyhow::Result;
use gix_protocol::handshake::Ref as HandshakeRef;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemoteRef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peeled: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

pub fn discover_remote_refs(repo_root: &Path, url: &str) -> Result<Vec<RemoteRef>> {
    let repo = gix::open(repo_root).with_context(|| {
        format!(
            "Failed to open repository context for remote ref discovery: {}",
            repo_root.display()
        )
    })?;
    let remote = repo
        .remote_at(url)
        .with_context(|| format!("Failed to create remote for URL: {url}"))?;
    let connection = remote
        .connect(gix::remote::Direction::Fetch)
        .with_context(|| format!("Failed to connect to remote: {url}"))?;
    // Disable server-side prefix filtering so we receive ALL refs.
    // With `remote_at()` there are no configured refspecs, so the default
    // behavior (prefix_from_spec_as_filter_on_remote: true) would cause the
    // server to return very few refs. Setting it to false ensures we get
    // the complete ref advertisement (branches, tags, HEAD, etc.).
    let options = gix::remote::ref_map::Options {
        prefix_from_spec_as_filter_on_remote: false,
        ..Default::default()
    };

    let (ref_map, _handshake) = connection
        .ref_map(gix::progress::Discard, options)
        .with_context(|| format!("Failed to list remote refs for: {url}"))?;

    Ok(ref_map
        .remote_refs
        .into_iter()
        .map(|remote_ref| match remote_ref {
            HandshakeRef::Direct {
                full_ref_name,
                object,
            } => RemoteRef {
                name: bytes_to_string(full_ref_name.as_ref()),
                oid: Some(object.to_string()),
                peeled: None,
                target: None,
            },
            HandshakeRef::Peeled {
                full_ref_name,
                tag,
                object,
            } => RemoteRef {
                name: bytes_to_string(full_ref_name.as_ref()),
                oid: Some(tag.to_string()),
                peeled: Some(object.to_string()),
                target: None,
            },
            HandshakeRef::Symbolic {
                full_ref_name,
                target,
                tag,
                object,
            } => RemoteRef {
                name: bytes_to_string(full_ref_name.as_ref()),
                oid: Some(tag.unwrap_or(object).to_string()),
                peeled: tag.map(|_| object.to_string()),
                target: Some(bytes_to_string(target.as_ref())),
            },
            HandshakeRef::Unborn {
                full_ref_name,
                target,
            } => RemoteRef {
                name: bytes_to_string(full_ref_name.as_ref()),
                oid: None,
                peeled: None,
                target: Some(bytes_to_string(target.as_ref())),
            },
        })
        .collect())
}

fn bytes_to_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
