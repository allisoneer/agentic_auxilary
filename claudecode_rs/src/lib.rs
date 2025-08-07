//! # ClaudeCode-RS
//!
//! A Rust SDK for programmatically interacting with Claude Code.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use claudecode::{Client, SessionConfig, Model, OutputFormat};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a client
//!     let client = Client::new().await?;
//!     
//!     // Simple query
//!     let config = SessionConfig::builder("Hello, Claude!")
//!         .model(Model::Sonnet)
//!         .build()?;
//!     
//!     let result = client.launch_and_wait(config).await?;
//!     println!("Claude says: {}", result.content.unwrap_or_default());
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Streaming Events
//!
//! ```rust,no_run
//! use claudecode::{Client, SessionConfig, OutputFormat, Event};
//! use futures::StreamExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = Client::new().await?;
//!     
//!     let config = SessionConfig::builder("Tell me a story")
//!         .output_format(OutputFormat::StreamingJson)
//!         .build()?;
//!     
//!     let mut session = client.launch(config).await?;
//!     
//!     // Process streaming events with type-safe pattern matching
//!     if let Some(mut events) = session.take_event_stream() {
//!         while let Some(event) = events.recv().await {
//!             match event {
//!                 Event::Assistant(msg) => {
//!                     println!("Claude: {:?}", msg.message.content);
//!                 }
//!                 Event::Result(result) => {
//!                     println!("Total cost: ${:?}", result.total_cost_usd);
//!                 }
//!                 _ => {}
//!             }
//!         }
//!     }
//!     
//!     let result = session.wait().await?;
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod config;
pub mod error;
pub mod process;
pub mod session;
pub mod stream;
pub mod types;

// Re-export main types
pub use client::Client;
pub use config::{MCPConfig, MCPServer, SessionConfig, SessionConfigBuilder};
pub use error::{ClaudeError, Result};
pub use session::Session;
pub use types::{
    AssistantEvent, Content, ErrorEvent, Event, MCPStatus, Message, Model, OutputFormat,
    Result as ClaudeResult, ResultEvent, ServerToolUse, SystemEvent, Usage,
};

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
