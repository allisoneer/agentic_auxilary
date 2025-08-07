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
        let content = if is_error {
            stderr_content.trim().to_string()
        } else {
            stdout_content.trim().to_string()
        };

        Ok(ClaudeResult {
            content: Some(content),
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
