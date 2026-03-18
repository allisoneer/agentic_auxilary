//! Integration tests for JSON extraction and validation.

use review_agent_mcp::types::ReviewLens;
use review_agent_mcp::validation::parse_and_validate_report;

#[test]
fn requires_caveat_when_confidence_medium() {
    let s = r#"{
      "lens":"security",
      "verdict":"needs_changes",
      "findings":[{
        "file":"x.rs",
        "line":1,
        "category":"security",
        "severity":"high",
        "confidence":"medium",
        "title":"test",
        "evidence":"test",
        "suggested_fix":"test"
      }],
      "notes":[]
    }"#;

    let err = parse_and_validate_report(s, ReviewLens::Security).unwrap_err();
    assert!(format!("{err:?}").contains("requires non-empty caveat"));
}

#[test]
fn accepts_caveat_when_confidence_medium() {
    let s = r#"{
      "lens":"security",
      "verdict":"needs_changes",
      "findings":[{
        "file":"x.rs",
        "line":1,
        "category":"security",
        "severity":"high",
        "confidence":"medium",
        "title":"test",
        "evidence":"test",
        "suggested_fix":"test",
        "caveat":"This might be a false positive because..."
      }],
      "notes":[]
    }"#;

    let report = parse_and_validate_report(s, ReviewLens::Security).unwrap();
    assert_eq!(report.findings.len(), 1);
}

#[test]
fn rejects_lens_mismatch() {
    let s = r#"{"lens":"security","verdict":"approved","findings":[],"notes":[]}"#;
    let err = parse_and_validate_report(s, ReviewLens::Correctness).unwrap_err();
    assert!(format!("{err:?}").contains("Lens mismatch"));
}

#[test]
fn rejects_category_mismatch() {
    let s = r#"{
      "lens":"security",
      "verdict":"needs_changes",
      "findings":[{
        "file":"x.rs",
        "line":1,
        "category":"correctness",
        "severity":"high",
        "confidence":"high",
        "title":"test",
        "evidence":"test",
        "suggested_fix":"test"
      }],
      "notes":[]
    }"#;

    let err = parse_and_validate_report(s, ReviewLens::Security).unwrap_err();
    assert!(format!("{err:?}").contains("does not match lens"));
}

#[test]
fn accepts_empty_findings() {
    let s = r#"{"lens":"testing","verdict":"approved","findings":[],"notes":["All good!"]}"#;
    let report = parse_and_validate_report(s, ReviewLens::Testing).unwrap();
    assert!(report.findings.is_empty());
    assert_eq!(report.notes.len(), 1);
}

#[test]
fn accepts_high_confidence_without_caveat() {
    let s = r#"{
      "lens":"maintainability",
      "verdict":"needs_changes",
      "findings":[{
        "file":"lib.rs",
        "line":42,
        "category":"maintainability",
        "severity":"medium",
        "confidence":"high",
        "title":"Function too complex",
        "evidence":"cyclomatic complexity > 10",
        "suggested_fix":"Split into smaller functions"
      }],
      "notes":[]
    }"#;

    let report = parse_and_validate_report(s, ReviewLens::Maintainability).unwrap();
    assert_eq!(report.findings.len(), 1);
    assert!(report.findings[0].caveat.is_none());
}
