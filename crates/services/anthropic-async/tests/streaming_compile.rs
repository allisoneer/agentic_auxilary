#[cfg(feature = "streaming")]
#[test]
fn streaming_feature_compiles() {
    use anthropic_async::streaming::{Accumulator, Event, EventStream, SSEDecoder, SseFrame};

    // Verify types are accessible and constructible by explicitly dropping instances.
    // The turbofish annotation ensures type resolution at compile time.
    drop::<SSEDecoder>(SSEDecoder::new());
    drop::<Accumulator>(Accumulator::new());
    drop::<SseFrame>(SseFrame::default());
    drop::<Event>(Event::MessageStop);
    drop::<Option<EventStream>>(None);
}

#[cfg(not(feature = "streaming"))]
#[test]
fn streaming_feature_not_enabled() {
    // Streaming should not be accessible without feature
}
