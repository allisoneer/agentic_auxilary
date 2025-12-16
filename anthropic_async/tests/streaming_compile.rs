#[cfg(feature = "streaming")]
#[test]
fn streaming_feature_compiles() {
    use anthropic_async::streaming::{Accumulator, Event, EventStream, SSEDecoder, SseFrame};

    // Just ensure types are accessible
    let _decoder = SSEDecoder::new();
    let _acc = Accumulator::new();
    let _frame = SseFrame::default();

    // Ensure Event::MessageStop is accessible (unit variant)
    let _: Event = Event::MessageStop;

    // Type annotation to ensure EventStream is properly defined
    let _: Option<EventStream> = None;
}

#[cfg(not(feature = "streaming"))]
#[test]
fn streaming_feature_not_enabled() {
    // Streaming should not be accessible without feature
}
