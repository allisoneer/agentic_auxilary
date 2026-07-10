//! Shared managed mise binary metadata.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BinarySpec {
    pub tool_name: &'static str,
    pub version_prefix: &'static str,
}

impl BinarySpec {
    pub const fn new(tool_name: &'static str, version_prefix: &'static str) -> Self {
        Self {
            tool_name,
            version_prefix,
        }
    }
}

pub const PLATFORM_TARGETS: [(&str, &str); 4] = [
    ("linux-x64", "x86_64-unknown-linux-gnu"),
    ("linux-arm64", "aarch64-unknown-linux-gnu"),
    ("macos-x64", "x86_64-apple-darwin"),
    ("macos-arm64", "aarch64-apple-darwin"),
];

pub const BINARY_SPECS: [BinarySpec; 4] = [
    BinarySpec::new("thoughts-bin", "thoughts-bin-v"),
    BinarySpec::new("agentic-bin", "agentic-bin-v"),
    BinarySpec::new("agentic-mcp", "agentic-mcp-v"),
    BinarySpec::new("opencode-orchestrator-mcp", "opencode-orchestrator-mcp-v"),
];

pub fn required_asset_names(tool_name: &str) -> Vec<String> {
    PLATFORM_TARGETS
        .iter()
        .map(|(_, target)| format!("{tool_name}-{target}.tar.xz"))
        .collect()
}

pub fn binary_spec(tool_name: &str) -> Option<&'static BinarySpec> {
    BINARY_SPECS.iter().find(|spec| spec.tool_name == tool_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_required_asset_names_for_managed_tool() {
        assert_eq!(
            required_asset_names("agentic-bin"),
            vec![
                "agentic-bin-x86_64-unknown-linux-gnu.tar.xz",
                "agentic-bin-aarch64-unknown-linux-gnu.tar.xz",
                "agentic-bin-x86_64-apple-darwin.tar.xz",
                "agentic-bin-aarch64-apple-darwin.tar.xz",
            ]
        );
    }

    #[test]
    fn finds_known_binary_spec() {
        assert_eq!(
            binary_spec("thoughts-bin"),
            Some(&BinarySpec::new("thoughts-bin", "thoughts-bin-v"))
        );
        assert_eq!(binary_spec("message-optimizer-bin"), None);
    }
}
