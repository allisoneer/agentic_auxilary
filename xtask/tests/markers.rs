use cargo_metadata::MetadataCommand;
use xtask::marker::{apply_autodeps_markers, RenderContext};

#[test]
fn malformed_json_is_warning_by_default() {
    let md = MetadataCommand::new().no_deps().exec().unwrap();
    let input = r#"<!-- BEGIN:autodeps {invalid} -->x<!-- END:autodeps -->"#;
    let (_out, _changed) = apply_autodeps_markers(
        input,
        &RenderContext {
            metadata: &md,
            strict: false,
        },
    )
    .unwrap();
}

#[test]
fn malformed_json_is_error_in_strict() {
    let md = MetadataCommand::new().no_deps().exec().unwrap();
    let input = r#"<!-- BEGIN:autodeps {invalid} -->x<!-- END:autodeps -->"#;
    let err = apply_autodeps_markers(
        input,
        &RenderContext {
            metadata: &md,
            strict: true,
        },
    )
    .unwrap_err();
    assert!(format!("{err}").contains("Malformed autodeps JSON"));
}

#[test]
fn unknown_crate_skips_block() {
    let md = MetadataCommand::new().no_deps().exec().unwrap();
    let input = r#"
<!-- BEGIN:autodeps {"crates":["this-crate-does-not-exist"]} -->
```toml
[dependencies]
this-crate-does-not-exist = "0.0.0"
```
<!-- END:autodeps -->
"#;
    let (out, changed) = apply_autodeps_markers(
        input,
        &RenderContext {
            metadata: &md,
            strict: false,
        },
    )
    .unwrap();
    assert!(!changed, "block should be left unchanged");
    assert!(out.contains("this-crate-does-not-exist = \"0.0.0\""));
}

#[test]
fn idempotent_on_second_run() {
    let md = MetadataCommand::new().no_deps().exec().unwrap();
    let input = r#"
<!-- BEGIN:autodeps {"crates":["universal-tool-core"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
universal-tool-core = "0.0.0"
```
<!-- END:autodeps -->
"#;
    let (out1, changed1) = apply_autodeps_markers(
        input,
        &RenderContext {
            metadata: &md,
            strict: false,
        },
    )
    .unwrap();
    assert!(changed1);
    let (out2, changed2) = apply_autodeps_markers(
        &out1,
        &RenderContext {
            metadata: &md,
            strict: false,
        },
    )
    .unwrap();
    assert!(!changed2);
    assert_eq!(out1, out2);
}

#[test]
fn updates_simple_block() {
    let md = MetadataCommand::new().no_deps().exec().unwrap();
    let input = r#"
Hello
<!-- BEGIN:autodeps {"crates":["universal-tool-core"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
universal-tool-core = "0.0.0"
```
<!-- END:autodeps -->
Bye
"#;
    let (out, changed) = apply_autodeps_markers(
        input,
        &RenderContext {
            metadata: &md,
            strict: false,
        },
    )
    .unwrap();
    assert!(changed);
    assert!(out.contains("universal-tool-core = \"0."));
}

#[test]
fn multiple_markers_in_file() {
    let md = MetadataCommand::new().no_deps().exec().unwrap();
    let input = r#"
First block:
<!-- BEGIN:autodeps {"crates":["universal-tool-core"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
universal-tool-core = "0.0.0"
```
<!-- END:autodeps -->

Second block:
<!-- BEGIN:autodeps {"crates":["claudecode"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
claudecode = "0.0.0"
```
<!-- END:autodeps -->
"#;
    let (out, changed) = apply_autodeps_markers(
        input,
        &RenderContext {
            metadata: &md,
            strict: false,
        },
    )
    .unwrap();
    assert!(changed);
    assert!(out.contains("universal-tool-core = \"0."));
    assert!(out.contains("claudecode = \"0."));
}

#[test]
fn preserves_content_outside_markers() {
    let md = MetadataCommand::new().no_deps().exec().unwrap();
    let input = r#"
# My Custom Content

This should not be changed.

<!-- BEGIN:autodeps {"crates":["universal-tool-core"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
universal-tool-core = "0.0.0"
```
<!-- END:autodeps -->

## More Custom Content

This should also remain unchanged.
"#;
    let (out, changed) = apply_autodeps_markers(
        input,
        &RenderContext {
            metadata: &md,
            strict: false,
        },
    )
    .unwrap();
    assert!(changed);
    assert!(out.contains("# My Custom Content"));
    assert!(out.contains("This should not be changed."));
    assert!(out.contains("## More Custom Content"));
    assert!(out.contains("This should also remain unchanged."));
}
