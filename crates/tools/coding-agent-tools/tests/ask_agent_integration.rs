//! Integration tests for ask_agent tool.
//! These tests are ignored by default as they require the claude CLI to be installed.
//! Run with: cargo test -p coding_agent_tools -- --ignored

use coding_agent_tools::CodingAgentTools;
use coding_agent_tools::types::{AgentLocation, AgentType};

#[tokio::test]
#[ignore]
async fn locator_codebase_basic() {
    let tools = CodingAgentTools::new();
    let out = tools
        .ask_agent(
            Some(AgentType::Locator),
            Some(AgentLocation::Codebase),
            "Find Cargo.toml files and related config".into(),
        )
        .await;

    match out {
        Ok(output) => {
            assert!(!output.text.trim().is_empty());
        }
        Err(e) => {
            panic!("ask_agent failed: {e}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn analyzer_web_basic() {
    let tools = CodingAgentTools::new();
    let out = tools
        .ask_agent(
            Some(AgentType::Analyzer),
            Some(AgentLocation::Web),
            "Summarize the core concepts of Rust error handling with sources".into(),
        )
        .await;

    match out {
        Ok(output) => {
            assert!(!output.text.trim().is_empty());
        }
        Err(e) => {
            panic!("ask_agent failed: {e}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn locator_web_basic() {
    let tools = CodingAgentTools::new();
    let out = tools
        .ask_agent(
            Some(AgentType::Locator),
            Some(AgentLocation::Web),
            "Find the official Rust documentation for the Result type".into(),
        )
        .await;

    match out {
        Ok(output) => {
            assert!(!output.text.trim().is_empty());
        }
        Err(e) => {
            panic!("ask_agent failed: {e}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn analyzer_codebase_basic() {
    let tools = CodingAgentTools::new();
    let out = tools
        .ask_agent(
            Some(AgentType::Analyzer),
            Some(AgentLocation::Codebase),
            "Analyze how the ls tool handles pagination in this codebase".into(),
        )
        .await;

    match out {
        Ok(output) => {
            assert!(!output.text.trim().is_empty());
        }
        Err(e) => {
            panic!("ask_agent failed: {e}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn ask_agent_empty_query_fails() {
    let tools = CodingAgentTools::new();
    let out = tools
        .ask_agent(
            Some(AgentType::Locator),
            Some(AgentLocation::Codebase),
            "   ".into(), // empty/whitespace only
        )
        .await;

    assert!(out.is_err());
    let err = out.unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[tokio::test]
#[ignore]
async fn ask_agent_defaults_to_locator_codebase() {
    let tools = CodingAgentTools::new();
    // Test that defaults work (locator + codebase)
    let out = tools
        .ask_agent(None, None, "Find test files in this project".into())
        .await;

    match out {
        Ok(output) => {
            assert!(!output.text.trim().is_empty());
        }
        Err(e) => {
            panic!("ask_agent with defaults failed: {e}");
        }
    }
}
