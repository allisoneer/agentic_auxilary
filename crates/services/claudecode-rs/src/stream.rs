use crate::error::{ClaudeError, Result};
use crate::types::{Event, Result as ClaudeResult};
use futures::{Stream, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tracing::trace;

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

    pub fn into_event_stream(self) -> impl Stream<Item = Result<Event>> {
        futures::stream::unfold(self, |mut parser| async move {
            let mut line = String::new();
            match parser.reader.read_line(&mut line).await {
                Ok(0) => None, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        // Continue to next line
                        return Some((Err(ClaudeError::StreamClosed), parser));
                    }

                    match serde_json::from_str::<Event>(trimmed) {
                        Ok(event) => {
                            trace!("Parsed event: {:?}", event);
                            Some((Ok(event), parser))
                        }
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

    pub async fn parse(mut self) -> Result<ClaudeResult> {
        let mut stdout_content = String::new();
        let mut stderr_content = String::new();

        // Read both streams
        self.stdout.read_to_string(&mut stdout_content).await?;
        self.stderr.read_to_string(&mut stderr_content).await?;

        // If stderr has content, this is an error
        if !stderr_content.trim().is_empty() {
            return Ok(ClaudeResult {
                is_error: true,
                error: Some(stderr_content.trim().to_string()),
                content: None,
                ..Default::default()
            });
        }

        // Parse stdout as JSON
        serde_json::from_str(&stdout_content).map_err(|e| ClaudeError::JsonParseError {
            source: e,
            line: None,
        })
    }
}

/// Parser for text output
pub struct TextParser<R1, R2> {
    stdout: BufReader<R1>,
    stderr: BufReader<R2>,
}

impl<R1: AsyncRead + Unpin, R2: AsyncRead + Unpin> TextParser<R1, R2> {
    pub fn new(stdout: R1, stderr: R2) -> Self {
        Self {
            stdout: BufReader::new(stdout),
            stderr: BufReader::new(stderr),
        }
    }

    pub async fn parse(mut self) -> Result<ClaudeResult> {
        let mut stdout_content = String::new();
        let mut stderr_content = String::new();

        // Read both streams
        self.stdout.read_to_string(&mut stdout_content).await?;
        self.stderr.read_to_string(&mut stderr_content).await?;

        // Determine if this is an error based on stderr content
        let is_error = !stderr_content.trim().is_empty();
        let chosen = if is_error {
            stderr_content.trim()
        } else {
            stdout_content.trim()
        };
        // Return None for empty/whitespace-only content instead of Some("")
        let content_opt = if chosen.is_empty() {
            None
        } else {
            Some(chosen.to_string())
        };

        Ok(ClaudeResult {
            content: content_opt,
            is_error,
            error: if is_error {
                Some("Process wrote to stderr".to_string())
            } else {
                None
            },
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod text_parser_tests {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::ReadBuf;

    /// Minimal AsyncRead adapter over in-memory bytes for tests
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

    #[tokio::test]
    async fn textparser_empty_stdout_returns_none() {
        let stdout = AsyncCursor::new(b"");
        let stderr = AsyncCursor::new(b"");
        let res = TextParser::new(stdout, stderr).parse().await.unwrap();
        assert!(!res.is_error);
        assert!(res.content.is_none());
    }

    #[tokio::test]
    async fn textparser_whitespace_stdout_returns_none() {
        let stdout = AsyncCursor::new(b" \n\t");
        let stderr = AsyncCursor::new(b"");
        let res = TextParser::new(stdout, stderr).parse().await.unwrap();
        assert!(!res.is_error);
        assert!(res.content.is_none());
    }

    #[tokio::test]
    async fn textparser_non_empty_stdout_returns_some() {
        let stdout = AsyncCursor::new(b"hello");
        let stderr = AsyncCursor::new(b"");
        let res = TextParser::new(stdout, stderr).parse().await.unwrap();
        assert!(!res.is_error);
        assert_eq!(res.content.as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn textparser_stderr_marks_error_and_returns_stderr() {
        let stdout = AsyncCursor::new(b"hello");
        let stderr = AsyncCursor::new(b"boom");
        let res = TextParser::new(stdout, stderr).parse().await.unwrap();
        assert!(res.is_error);
        assert_eq!(res.content.as_deref(), Some("boom"));
        assert!(
            res.error
                .as_deref()
                .unwrap_or("")
                .contains("Process wrote to stderr")
        );
    }
}
