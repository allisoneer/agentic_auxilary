use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

#[test]
fn checked_in_codec_fixtures_are_canonical_json_with_measured_sizes() -> TestResult {
    let event = include_bytes!("fixtures/codec/event_v1.json");
    let event_value: serde_json::Value = serde_json::from_slice(event)?;
    assert!(event.ends_with(b"\n"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&serde_json::to_vec(&event_value)?)?,
        event_value
    );
    assert!(
        event.len() > 1_000,
        "event fixture unexpectedly lost complete views"
    );

    let outcomes = include_str!("fixtures/codec/outcomes_v1.jsonl");
    let lines: Vec<_> = outcomes.lines().collect();
    assert_eq!(lines.len(), 9);
    for (index, line) in lines.iter().enumerate() {
        let value: serde_json::Value = serde_json::from_str(line)?;
        assert_eq!(value["operation"], index);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&serde_json::to_string(&value)?)?,
            value
        );
        assert!(line.len() < 512, "fixture {index} grew unexpectedly");
    }
    Ok(())
}
