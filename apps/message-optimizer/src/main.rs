use std::io::Read;

use anyhow::{Context, Result};
use clap::Parser;
use message_optimizer::{OptimizeMessageRequest, OptimizedPrompt, optimize_message};

#[derive(Debug, Parser)]
#[command(name = "message-optimizer")]
#[command(about = "Optimize a single message into a GPT-5.4 prompt")]
#[command(version)]
struct Cli {
    message: String,

    #[arg(long)]
    supplemental_context: Option<String>,

    #[arg(long)]
    json: bool,

    #[arg(long, requires = "json")]
    pretty: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let message = read_message(cli.message)?;
    let optimized = optimize_message(OptimizeMessageRequest {
        message,
        supplemental_context: cli.supplemental_context,
    })
    .await?;

    print!("{}", render_output(&optimized, cli.json, cli.pretty)?);
    Ok(())
}

fn read_message(message: String) -> Result<String> {
    if message != "-" {
        return Ok(message);
    }

    let mut stdin = std::io::stdin();
    let mut buffer = String::new();
    stdin
        .read_to_string(&mut buffer)
        .context("failed to read message from stdin")?;
    Ok(buffer)
}

fn render_output(prompt: &OptimizedPrompt, json: bool, pretty: bool) -> Result<String> {
    if !json {
        return Ok(prompt.assembled_prompt.clone());
    }

    if pretty {
        return serde_json::to_string_pretty(prompt).context("failed to serialize pretty JSON");
    }

    serde_json::to_string(prompt).context("failed to serialize JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_output_defaults_to_assembled_prompt() {
        let prompt = OptimizedPrompt {
            system_prompt: "system".to_string(),
            user_prompt: "user".to_string(),
            assembled_prompt: "assembled".to_string(),
        };

        assert_eq!(
            render_output(&prompt, false, false).ok(),
            Some("assembled".to_string())
        );
    }

    #[test]
    fn render_output_supports_pretty_json() {
        let prompt = OptimizedPrompt {
            system_prompt: "system".to_string(),
            user_prompt: "user".to_string(),
            assembled_prompt: "assembled".to_string(),
        };

        let rendered = render_output(&prompt, true, true);
        assert!(matches!(rendered, Ok(value) if value.contains("\n  \"system_prompt\"")));
    }
}
