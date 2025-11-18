use anthropic_async::{
    types::{
        common::CacheControl,
        content::{
            ContentBlockParam, DocumentSource, ImageSource, MessageContentParam, MessageParam,
            MessageRole,
        },
        messages::MessagesCreateRequest,
    },
    AnthropicConfig, Client,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::with_config(AnthropicConfig::new());

    // Create an image content block from a URL
    let image = ContentBlockParam::Image {
        source: ImageSource::Url {
            url: "https://upload.wikimedia.org/wikipedia/commons/a/a7/Camponotus_flavomarginatus_ant.jpg".into(),
        },
        cache_control: None,
    };

    // Create a document content block with base64 encoding
    // (This is a placeholder - in real usage, you'd encode actual document data)
    let document = ContentBlockParam::Document {
        source: DocumentSource::Base64 {
            media_type: "application/pdf".into(),
            data: "JVBERi0xLjQKJeLjz9MK...".into(), // Truncated base64 PDF
        },
        cache_control: Some(CacheControl::ephemeral_1h()),
    };

    // Create a text block asking about the multimodal content
    let text = ContentBlockParam::Text {
        text: "What's in this image and document?".into(),
        cache_control: None,
    };

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet-20241022".into(),
        max_tokens: 1024,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: MessageContentParam::Blocks(vec![image, document, text]),
        }],
        system: None,
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    println!("Sending multimodal request with image and document...");
    let response = client.messages().create(req).await?;

    println!("\nResponse:");
    for block in &response.content {
        match block {
            anthropic_async::types::ContentBlock::Text { text } => {
                println!("{}", text);
            }
            anthropic_async::types::ContentBlock::ToolUse { name, .. } => {
                println!("Tool called: {}", name);
            }
        }
    }

    if let Some(usage) = response.usage {
        println!("\nToken usage:");
        if let Some(input) = usage.input_tokens {
            println!("  Input tokens: {}", input);
        }
        if let Some(output) = usage.output_tokens {
            println!("  Output tokens: {}", output);
        }
    }

    Ok(())
}
