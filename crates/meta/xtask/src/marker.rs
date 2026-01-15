use anyhow::{Context, Result};
use cargo_metadata::Metadata;
use regex::Regex;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct BlockConfig {
    pub crates: Vec<String>,
    #[serde(default)]
    pub fence: Option<String>,
    #[serde(default)]
    pub header: Option<String>,
}

pub struct RenderContext<'a> {
    pub metadata: &'a Metadata,
    pub strict: bool,
}

fn version_map(metadata: &Metadata) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pkg in &metadata.packages {
        if metadata.workspace_members.contains(&pkg.id) {
            map.insert(pkg.name.clone(), pkg.version.to_string());
        }
    }
    map
}

pub fn apply_autodeps_markers(input: &str, ctx: &RenderContext) -> Result<(String, bool)> {
    let re = Regex::new(
        r#"(?s)<!--\s*BEGIN:autodeps\s*(\{.*?\})\s*-->\s*(.*?)\s*<!--\s*END:autodeps\s*-->"#,
    )
    .context("compile autodeps regex")?;

    let vmap = version_map(ctx.metadata);

    let mut changed = false;
    let mut out = String::with_capacity(input.len());
    let mut last = 0usize;

    for caps in re.captures_iter(input) {
        let m = caps.get(0).unwrap();
        let cfg_json = caps.get(1).unwrap().as_str();

        // Copy content before this block
        out.push_str(&input[last..m.start()]);

        let cfg: BlockConfig = match serde_json::from_str(cfg_json) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[autodeps] WARN: malformed JSON in marker: {e}");
                if ctx.strict {
                    return Err(anyhow::anyhow!("Malformed autodeps JSON: {e}"));
                }
                // leave block unchanged
                out.push_str(m.as_str());
                last = m.end();
                continue;
            }
        };

        // Validate crates exist
        let mut unknown: Vec<String> = Vec::new();
        for name in &cfg.crates {
            if !vmap.contains_key(name) {
                unknown.push(name.clone());
            }
        }
        if !unknown.is_empty() {
            eprintln!(
                "[autodeps] WARN: unknown crates in marker: {}",
                unknown.join(", ")
            );
            if ctx.strict {
                return Err(anyhow::anyhow!(
                    "Unknown crates in autodeps marker: {}",
                    unknown.join(", ")
                ));
            }
            // leave this block unchanged
            out.push_str(m.as_str());
            last = m.end();
            continue;
        }

        // Render replacement block
        let fence = cfg.fence.as_deref().unwrap_or("toml");
        let mut rendered = String::new();
        rendered.push_str(&format!("<!-- BEGIN:autodeps {cfg_json} -->\n"));
        rendered.push_str(&format!("```{fence}\n"));
        if let Some(header) = cfg.header.as_deref() {
            rendered.push_str(header);
            rendered.push('\n');
        }
        for name in &cfg.crates {
            let ver = &vmap[name];
            rendered.push_str(&format!(r#"{name} = "{ver}""#));
            rendered.push('\n');
        }
        rendered.push_str("```\n");
        rendered.push_str("<!-- END:autodeps -->");

        if m.as_str() != rendered {
            changed = true;
        }
        out.push_str(&rendered);
        last = m.end();
    }

    // Copy any trailing content
    out.push_str(&input[last..]);

    Ok((out, changed))
}
