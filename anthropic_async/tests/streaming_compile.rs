#[cfg(feature = "streaming")]
#[test]
fn streaming_feature_compiles() {
    use anthropic_async::sse::streaming::{Event, EventStream};

    // Just ensure types are accessible
    drop(Event::MessageStart);

    // Type annotation to ensure EventStream is properly defined
    let _: Option<EventStream> = None;
}

#[cfg(not(feature = "streaming"))]
#[test]
fn streaming_feature_not_enabled() {
    // Streaming should not be accessible without feature
}
