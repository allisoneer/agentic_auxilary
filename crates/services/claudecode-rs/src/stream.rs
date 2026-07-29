use crate::error::ClaudeError;
use crate::error::Result;
use crate::types::Event;
use crate::types::RawEvent;
use crate::types::Result as ClaudeResult;
use futures::Stream;
use futures::StreamExt;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;

pub(crate) const DIAGNOSTIC_BYTE_LIMIT: usize = 256 * 1024;
pub(crate) const EVENT_LINE_BYTE_LIMIT: usize = 1024 * 1024;

fn bounded_lossy_tail(bytes: &[u8], limit: usize) -> String {
    let mut value = String::from_utf8_lossy(bytes).into_owned();
    if value.len() > limit {
        let mut start = value.len() - limit;
        while !value.is_char_boundary(start) {
            start += 1;
        }
        value.drain(..start);
    }
    value
}

async fn read_bounded_line<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> std::io::Result<Option<(Vec<u8>, bool)>> {
    let mut line = Vec::new();
    let mut oversized = false;
    let mut read_any = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(read_any.then_some((line, oversized)));
        }
        read_any = true;
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if consumed >= EVENT_LINE_BYTE_LIMIT {
            line.clear();
            line.extend_from_slice(&available[consumed - EVENT_LINE_BYTE_LIMIT..consumed]);
            oversized = true;
        } else {
            if line.len() + consumed > EVENT_LINE_BYTE_LIMIT {
                let discard = line.len() + consumed - EVENT_LINE_BYTE_LIMIT;
                line.drain(..discard);
                oversized = true;
            }
            line.extend_from_slice(&available[..consumed]);
        }
        let complete = available[..consumed].ends_with(b"\n");
        reader.consume(consumed);
        if complete {
            return Ok(Some((line, oversized)));
        }
    }
}

pub(crate) async fn read_bounded_tail<R: AsyncRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> std::io::Result<(String, bool)> {
    let mut tail = Vec::with_capacity(limit.min(8192));
    let mut chunk = vec![0_u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        if count >= limit {
            tail.clear();
            tail.extend_from_slice(&chunk[count - limit..count]);
            truncated = true;
            continue;
        }
        if tail.len() + count > limit {
            let discard = tail.len() + count - limit;
            tail.drain(..discard);
            truncated = true;
        }
        tail.extend_from_slice(&chunk[..count]);
    }
    Ok((bounded_lossy_tail(&tail, limit), truncated))
}

/// Parser for streaming JSON events (NDJSON format)
pub struct JsonStreamParser<R> {
    reader: BufReader<R>,
}

impl<R: AsyncRead + Unpin> JsonStreamParser<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
        }
    }

    pub fn into_event_stream(self) -> impl Stream<Item = Result<RawEvent>> {
        futures::stream::unfold(self, |mut parser| async move {
            match read_bounded_line(&mut parser.reader).await {
                Ok(None) => None,
                Ok(Some((line, oversized))) => {
                    let line = bounded_lossy_tail(&line, EVENT_LINE_BYTE_LIMIT);
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        // Continue to next line
                        return Some((Err(ClaudeError::StreamClosed), parser));
                    }

                    if oversized {
                        return Some((
                            Err(ClaudeError::EventLineTooLong {
                                limit: EVENT_LINE_BYTE_LIMIT,
                                tail: line,
                            }),
                            parser,
                        ));
                    }

                    match serde_json::from_str::<serde_json::Value>(trimmed)
                        .and_then(RawEvent::from_value)
                    {
                        Ok(event) => Some((Ok(event), parser)),
                        Err(e) => Some((
                            Err(ClaudeError::JsonParseError {
                                source: e,
                                line: Some(trimmed.to_string()),
                            }),
                            parser,
                        )),
                    }
                }
                Err(e) => Some((Err(e.into()), parser)),
            }
        })
        .filter_map(|result| async move {
            // Filter out empty line errors
            match result {
                Err(ClaudeError::StreamClosed) => None,
                other => Some(other),
            }
        })
    }
}

#[derive(Debug)]
pub struct ParsedJsonOutput {
    pub result: Option<ClaudeResult>,
    pub error: Option<ClaudeError>,
    pub events: Vec<RawEvent>,
    pub raw_stdout: String,
    pub stderr: String,
}

pub fn normalize_result_event(event: &crate::types::ResultEvent) -> ClaudeResult {
    ClaudeResult {
        result_type: Some("result".to_string()),
        subtype: event.subtype.clone(),
        session_id: Some(event.session_id.clone()),
        result: event.result.clone(),
        content: event.result.clone(),
        is_error: event.is_error,
        error: event.error.clone(),
        total_cost_usd: event.total_cost_usd,
        duration_ms: event.duration_ms,
        duration_api_ms: event.duration_api_ms,
        num_turns: event.num_turns,
        exit_code: None,
        usage: event.usage.clone(),
    }
}

pub fn normalize_error_event(event: &crate::types::ErrorEvent) -> ClaudeResult {
    ClaudeResult {
        result_type: Some("error".to_string()),
        session_id: Some(event.session_id.clone()),
        is_error: true,
        error: Some(event.error.clone()),
        ..Default::default()
    }
}

fn parse_json_output(stdout: &str) -> Result<(ClaudeResult, Vec<RawEvent>)> {
    let trimmed = stdout.trim();
    if let Ok(value @ serde_json::Value::Object(_)) = serde_json::from_str(trimmed)
        && value.get("type").is_none()
        && is_credible_untagged_terminal_result(&value)
    {
        let mut result = serde_json::from_value::<ClaudeResult>(value)?;
        if result.content.is_none() {
            result.content.clone_from(&result.result);
        }
        if result.result.is_none() {
            result.result.clone_from(&result.content);
        }
        return Ok((result, Vec::new()));
    }
    let roots = match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(serde_json::Value::Array(values)) => values,
        Ok(value) => vec![value],
        Err(_) => trimmed
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).map_err(|source| {
                    ClaudeError::JsonParseError {
                        source,
                        line: Some(line.to_string()),
                    }
                })
            })
            .collect::<Result<Vec<_>>>()?,
    };

    let mut events = Vec::with_capacity(roots.len());
    let mut final_result = None;
    for value in roots {
        let envelope = RawEvent::from_value(value)
            .map_err(|source| ClaudeError::JsonParseError { source, line: None })?;
        if let Event::Result(result_event) = &envelope.event {
            final_result = Some(normalize_result_event(result_event));
        } else if let Event::Error(error_event) = &envelope.event {
            final_result = Some(normalize_error_event(error_event));
        }
        events.push(envelope);
    }

    let result = final_result.ok_or_else(|| ClaudeError::SessionError {
        message: "Claude JSON output contained no terminal result event".to_string(),
    })?;
    Ok((result, events))
}

fn is_credible_untagged_terminal_result(value: &serde_json::Value) -> bool {
    ["result", "content", "error"]
        .iter()
        .any(|key| value.get(key).is_some())
        || value
            .get("is_error")
            .and_then(serde_json::Value::as_bool)
            .is_some()
        || value
            .get("status")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|status| {
                matches!(
                    status.to_ascii_lowercase().as_str(),
                    "success"
                        | "succeeded"
                        | "complete"
                        | "completed"
                        | "done"
                        | "error"
                        | "failed"
                        | "failure"
                        | "cancelled"
                        | "canceled"
                )
            })
}

/// Parser for single JSON response
pub struct SingleJsonParser<R1, R2> {
    stdout: BufReader<R1>,
    stderr: BufReader<R2>,
}

impl<R1: AsyncRead + Unpin, R2: AsyncRead + Unpin> SingleJsonParser<R1, R2> {
    pub fn new(stdout: R1, stderr: R2) -> Self {
        Self {
            stdout: BufReader::new(stdout),
            stderr: BufReader::new(stderr),
        }
    }

    pub async fn parse(mut self) -> Result<ParsedJsonOutput> {
        // Drain both streams concurrently so stderr backpressure cannot block stdout EOF.
        let (stdout, stderr) = tokio::join!(
            read_bounded_tail(&mut self.stdout, EVENT_LINE_BYTE_LIMIT),
            read_bounded_tail(&mut self.stderr, DIAGNOSTIC_BYTE_LIMIT)
        );
        let (stdout_content, stdout_truncated) = stdout?;
        let (stderr_content, _) = stderr?;
        let parsed = if stdout_truncated {
            Err(ClaudeError::EventLineTooLong {
                limit: EVENT_LINE_BYTE_LIMIT,
                tail: stdout_content.clone(),
            })
        } else {
            parse_json_output(&stdout_content)
        };
        let (result, events, error) = match parsed {
            Ok((result, events)) => (Some(result), events, None),
            Err(error) => (None, Vec::new(), Some(error)),
        };
        Ok(ParsedJsonOutput {
            result,
            error,
            events,
            raw_stdout: stdout_content,
            stderr: stderr_content,
        })
    }
}

/// Parser for text output
pub struct TextParser<R1, R2> {
    stdout: BufReader<R1>,
    stderr: BufReader<R2>,
}

#[derive(Debug, Clone)]
pub struct ParsedTextOutput {
    pub result: ClaudeResult,
    pub raw_stdout: String,
    pub stderr: String,
}

impl<R1: AsyncRead + Unpin, R2: AsyncRead + Unpin> TextParser<R1, R2> {
    pub fn new(stdout: R1, stderr: R2) -> Self {
        Self {
            stdout: BufReader::new(stdout),
            stderr: BufReader::new(stderr),
        }
    }

    pub async fn parse(mut self) -> Result<ParsedTextOutput> {
        // Drain both streams concurrently so stderr backpressure cannot block stdout EOF.
        let (stdout, stderr) = tokio::join!(
            read_bounded_tail(&mut self.stdout, DIAGNOSTIC_BYTE_LIMIT),
            read_bounded_tail(&mut self.stderr, DIAGNOSTIC_BYTE_LIMIT)
        );
        let (stdout_content, _) = stdout?;
        let (stderr_content, _) = stderr?;

        let stdout = stdout_content.trim();
        let content = if stdout.is_empty() {
            None
        } else {
            Some(stdout.to_string())
        };

        Ok(ParsedTextOutput {
            result: ClaudeResult {
                content,
                ..Default::default()
            },
            raw_stdout: stdout_content,
            stderr: stderr_content,
        })
    }
}

#[cfg(test)]
mod text_parser_tests {
    use super::*;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::io::DuplexStream;
    use tokio::io::ReadBuf;
    use tokio::time::timeout;

    const PIPE_BUFFER_SIZE: usize = 64;
    const LARGE_STDERR_LEN: usize = 128 * 1024;
    const PARSE_TIMEOUT: Duration = Duration::from_secs(2);

    #[test]
    fn parsed_events_have_no_payload_trace_logging_site() {
        let source = include_str!("stream.rs");
        assert!(!source.contains(concat!("trace", "!(")));
        assert!(!source.contains(concat!("Parsed", " event")));
    }

    /// Minimal `AsyncRead` adapter over in-memory bytes for tests
    struct AsyncCursor {
        inner: std::io::Cursor<Vec<u8>>,
    }

    impl AsyncCursor {
        fn new(data: impl AsRef<[u8]>) -> Self {
            Self {
                inner: std::io::Cursor::new(data.as_ref().to_vec()),
            }
        }
    }

    impl Unpin for AsyncCursor {}

    impl AsyncRead for AsyncCursor {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            if self.inner.position() as usize >= self.inner.get_ref().len() {
                return Poll::Ready(Ok(()));
            }
            let mut temp = vec![0u8; buf.remaining()];
            let n = std::io::Read::read(&mut self.inner, &mut temp[..]).unwrap_or(0);
            buf.put_slice(&temp[..n]);
            Poll::Ready(Ok(()))
        }
    }

    fn streams_with_stdout_open_until_stderr_written(
        stdout_content: &'static [u8],
        stderr_content: Vec<u8>,
    ) -> (
        DuplexStream,
        DuplexStream,
        tokio::task::JoinHandle<std::io::Result<()>>,
    ) {
        let (stdout_reader, mut stdout_writer) = tokio::io::duplex(PIPE_BUFFER_SIZE);
        let (stderr_reader, mut stderr_writer) = tokio::io::duplex(PIPE_BUFFER_SIZE);

        let writer = tokio::spawn(async move {
            stdout_writer.write_all(stdout_content).await?;
            stderr_writer.write_all(&stderr_content).await?;
            stderr_writer.shutdown().await?;
            stdout_writer.shutdown().await?;
            Ok(())
        });

        (stdout_reader, stderr_reader, writer)
    }

    #[tokio::test]
    async fn textparser_empty_stdout_returns_none() {
        let stdout = AsyncCursor::new(b"");
        let stderr = AsyncCursor::new(b"");
        let res = TextParser::new(stdout, stderr).parse().await.unwrap();
        assert!(!res.result.is_error);
        assert!(res.result.content.is_none());
    }

    #[tokio::test]
    async fn textparser_whitespace_stdout_returns_none() {
        let stdout = AsyncCursor::new(b" \n\t");
        let stderr = AsyncCursor::new(b"");
        let res = TextParser::new(stdout, stderr).parse().await.unwrap();
        assert!(!res.result.is_error);
        assert!(res.result.content.is_none());
    }

    #[tokio::test]
    async fn textparser_non_empty_stdout_returns_some() {
        let stdout = AsyncCursor::new(b"hello");
        let stderr = AsyncCursor::new(b"");
        let res = TextParser::new(stdout, stderr).parse().await.unwrap();
        assert!(!res.result.is_error);
        assert_eq!(res.result.content.as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn textparser_warning_stderr_remains_diagnostic() {
        let stdout = AsyncCursor::new(b"hello");
        let stderr = AsyncCursor::new(b"boom");
        let res = TextParser::new(stdout, stderr).parse().await.unwrap();
        assert!(!res.result.is_error);
        assert_eq!(res.result.content.as_deref(), Some("hello"));
        assert_eq!(res.raw_stdout, "hello");
        assert_eq!(res.stderr, "boom");
    }

    #[tokio::test]
    async fn textparser_drains_stderr_while_stdout_remains_open() {
        let stderr_content = vec![b'e'; LARGE_STDERR_LEN];
        let (stdout, stderr, writer) =
            streams_with_stdout_open_until_stderr_written(b"stdout stays open", stderr_content);

        let parsed = timeout(PARSE_TIMEOUT, TextParser::new(stdout, stderr).parse()).await;
        if parsed.is_err() {
            writer.abort();
        }
        let res = parsed.expect("text parser timed out").unwrap();
        writer.await.unwrap().unwrap();

        assert!(!res.result.is_error);
        assert_eq!(res.result.content.as_deref(), Some("stdout stays open"));
        assert_eq!(res.stderr.len(), LARGE_STDERR_LEN);
    }

    #[tokio::test]
    async fn singlejsonparser_stdout_json_returns_result() {
        let stdout = AsyncCursor::new(
            br#"{"type":"result","session_id":"s","result":"ok","is_error":false}"#,
        );
        let stderr = AsyncCursor::new(b"");
        let res = SingleJsonParser::new(stdout, stderr).parse().await.unwrap();
        assert!(!res.result.as_ref().unwrap().is_error);
        assert_eq!(res.result.unwrap().result.as_deref(), Some("ok"));
    }

    #[tokio::test]
    async fn singlejsonparser_warning_stderr_remains_diagnostic() {
        let stdout = AsyncCursor::new(
            br#"{"type":"result","session_id":"s","result":"ok","is_error":false}"#,
        );
        let stderr = AsyncCursor::new(b" warning ");
        let res = SingleJsonParser::new(stdout, stderr).parse().await.unwrap();
        assert!(!res.result.unwrap().is_error);
        assert_eq!(res.stderr.trim(), "warning");
    }

    #[tokio::test]
    async fn singlejsonparser_drains_stderr_while_stdout_remains_open() {
        let stderr_content = vec![b'e'; LARGE_STDERR_LEN];
        let (stdout, stderr, writer) = streams_with_stdout_open_until_stderr_written(
            br#"{"type":"result","session_id":"s","result":"ok","is_error":false}"#,
            stderr_content,
        );

        let parsed = timeout(PARSE_TIMEOUT, SingleJsonParser::new(stdout, stderr).parse()).await;
        if parsed.is_err() {
            writer.abort();
        }
        let res = parsed.expect("single-json parser timed out").unwrap();
        writer.await.unwrap().unwrap();

        assert!(!res.result.unwrap().is_error);
        assert_eq!(res.stderr.len(), LARGE_STDERR_LEN);
    }

    #[test]
    fn parses_verbose_array_and_ndjson_without_dropping_unknown_events() {
        let array =
            r#"[{"type":"future","nonce":1},{"type":"result","session_id":"s","result":"done"}]"#;
        let (result, events) = parse_json_output(array).unwrap();
        assert_eq!(result.content.as_deref(), Some("done"));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].raw["nonce"], 1);

        let ndjson = "{\"type\":\"future\",\"nonce\":2}\n{\"type\":\"result\",\"session_id\":\"s\",\"result\":\"done\"}\n";
        let (_, events) = parse_json_output(ndjson).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].raw["nonce"], 2);
    }

    #[test]
    fn parses_single_untagged_result_and_normalizes_content() {
        let (result, events) =
            parse_json_output(r#"{"session_id":"s","result":"untagged result","is_error":false}"#)
                .unwrap();
        assert!(events.is_empty());
        assert_eq!(result.result.as_deref(), Some("untagged result"));
        assert_eq!(result.content.as_deref(), Some("untagged result"));
    }

    #[test]
    fn unknown_untagged_warning_object_is_not_a_successful_default_result() {
        let error = parse_json_output(
            r#"{"status":"warning","message":"tools may be unavailable","tools":[]}"#,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ClaudeError::JsonParseError { .. } | ClaudeError::SessionError { .. }
        ));
    }

    #[test]
    fn explicit_untagged_terminal_status_is_accepted() {
        for status in ["completed", "failed"] {
            let output = serde_json::json!({"status": status, "session_id": "s"}).to_string();
            let (result, events) = parse_json_output(&output).unwrap();
            assert!(events.is_empty());
            assert_eq!(result.session_id.as_deref(), Some("s"));
        }
    }

    #[test]
    fn tagged_terminal_error_is_classified_as_a_result() {
        let (result, events) = parse_json_output(
            r#"[{"type":"system","session_id":"s"},{"type":"error","session_id":"s","error":"terminal failure"}]"#,
        )
        .unwrap();
        assert_eq!(events.len(), 2);
        assert!(result.is_error);
        assert_eq!(result.error.as_deref(), Some("terminal failure"));
    }

    #[tokio::test]
    async fn oversized_text_streams_are_bounded_while_reading() {
        let stdout = AsyncCursor::new(vec![b'o'; DIAGNOSTIC_BYTE_LIMIT * 3]);
        let stderr = AsyncCursor::new(vec![b'e'; DIAGNOSTIC_BYTE_LIMIT * 3]);
        let parsed = TextParser::new(stdout, stderr).parse().await.unwrap();
        assert_eq!(parsed.raw_stdout.len(), DIAGNOSTIC_BYTE_LIMIT);
        assert_eq!(parsed.stderr.len(), DIAGNOSTIC_BYTE_LIMIT);
        assert_eq!(parsed.result.content.unwrap().len(), DIAGNOSTIC_BYTE_LIMIT);
    }
}
