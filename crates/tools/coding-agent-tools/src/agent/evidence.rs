use claudecode::Content;
use claudecode::Event;
use claudecode::SessionOutcome;
use std::collections::HashMap;
use std::collections::HashSet;

fn tool_use(block: &Content) -> Option<(&str, &str)> {
    match block {
        Content::ToolUse { id, name, .. } | Content::StructuredToolUse { id, name, .. } => {
            Some((id, name))
        }
        _ => None,
    }
}

fn walk_transcript_pairs<U, P>(
    outcome: &SessionOutcome,
    mut on_tool_use: U,
    mut on_pair: P,
) -> Result<(), String>
where
    U: FnMut(&str, &str) -> Result<(), String>,
    P: FnMut(&str, &str, serde_json::Value, bool) -> Result<(), String>,
{
    let mut uses = HashMap::<String, String>::new();
    for envelope in &outcome.transcript {
        match &envelope.event {
            Event::Assistant(event) => {
                if event.message.role != "assistant" {
                    return Err(
                        "Assistant event contained a non-assistant message role".to_string()
                    );
                }
                for block in &event.message.content {
                    let Some((id, name)) = tool_use(block) else {
                        if block.tool_result().is_some() {
                            return Err("Tool result appeared outside a user event".to_string());
                        }
                        continue;
                    };
                    on_tool_use(id, name)?;
                    if uses.insert(id.to_string(), name.to_string()).is_some() {
                        return Err(format!("Transcript reused tool-use ID `{id}`"));
                    }
                }
            }
            Event::User(event) => {
                if event.message.role != "user" {
                    return Err("User event contained a non-user message role".to_string());
                }
                for block in &event.message.content {
                    if tool_use(block).is_some() {
                        return Err("Tool use appeared outside an assistant event".to_string());
                    }
                    if let Some((tool_use_id, content, is_error)) = block.tool_result() {
                        let Some(name) = uses.remove(tool_use_id) else {
                            return Err(format!(
                                "Tool result `{tool_use_id}` did not follow a matching assistant tool use"
                            ));
                        };
                        on_pair(tool_use_id, &name, content, is_error)?;
                    }
                }
            }
            _ => {}
        }
    }
    if !uses.is_empty() {
        let mut ids = uses.into_keys().collect::<Vec<_>>();
        ids.sort();
        return Err(format!(
            "Claude transcript contained unpaired assistant tool uses: {ids:?}"
        ));
    }
    Ok(())
}

pub fn validate_outcome(
    outcome: &SessionOutcome,
    allowed_tools: &[String],
) -> Result<String, String> {
    if outcome.exit_code != Some(0) {
        return Err(format!("Claude exited with status {:?}", outcome.exit_code));
    }
    if outcome.result.is_error {
        return Err(outcome
            .result
            .error
            .clone()
            .unwrap_or_else(|| "Claude returned a terminal error".to_string()));
    }
    let allowed = allowed_tools
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let init = outcome
        .transcript
        .iter()
        .find_map(|envelope| match &envelope.event {
            Event::System(system) if system.subtype.as_deref() == Some("init") => Some(system),
            _ => None,
        })
        .ok_or_else(|| "Claude transcript contained no init event".to_string())?;
    if !init.mcp_server_errors.is_empty() {
        return Err(format!(
            "MCP server initialization errors: {:?}",
            init.mcp_server_errors
        ));
    }
    let initialized = init
        .tools
        .as_ref()
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if initialized != allowed {
        return Err(format!(
            "Claude init tool mismatch: expected {allowed:?}, found {initialized:?}"
        ));
    }

    let mut has_evidence = false;
    walk_transcript_pairs(
        outcome,
        |_, name| {
            if !allowed.contains(name) {
                return Err(format!("Transcript called disallowed tool `{name}`"));
            }
            Ok(())
        },
        |tool_use_id, name, _, is_error| {
            if is_error {
                return Err(format!(
                    "Tool result `{tool_use_id}` for `{name}` reported an error"
                ));
            }
            if name != "mcp__agentic-mcp__workspace_todowrite" {
                has_evidence = true;
            }
            Ok(())
        },
    )?;
    if !has_evidence {
        return Err(
            "Claude transcript contained no allowed non-todo tool use with a matching successful result"
                .to_string(),
        );
    }
    outcome
        .result
        .result
        .as_ref()
        .or(outcome.result.content.as_ref())
        .filter(|text| !text.trim().is_empty())
        .cloned()
        .ok_or_else(|| "Claude session produced no final text".to_string())
}

pub fn validate_nonce_provenance(
    outcome: &SessionOutcome,
    expected_tools: &[String],
    prompt_parts: &[&str],
    nonce: &str,
) -> Result<(), String> {
    if nonce.is_empty() || prompt_parts.iter().any(|part| part.contains(nonce)) {
        return Err(
            "Evidence nonce must be non-empty and absent from every composed prompt part"
                .to_string(),
        );
    }
    let expected = expected_tools
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if expected.is_empty() {
        return Err("Expected evidence tool list must not be empty".to_string());
    }
    let mut found = false;
    walk_transcript_pairs(
        outcome,
        |_, _| Ok(()),
        |_, name, content, is_error| {
            if !is_error && expected.contains(name) && content.to_string().contains(nonce) {
                found = true;
            }
            Ok(())
        },
    )?;
    if found {
        Ok(())
    } else {
        Err("Nonce was absent from successful paired expected tool-result evidence".to_string())
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests should fail immediately on fixture and assertion errors"
)]
mod tests {
    use super::*;

    fn outcome(tool_name: &str, include_result: bool, result_error: bool) -> SessionOutcome {
        let init = serde_json::json!({
            "type": "system", "subtype": "init", "session_id": "s",
            "tools": [tool_name], "mcp_server_errors": []
        });
        let call = serde_json::json!({
            "type": "assistant", "session_id": "s", "message": {
                "role": "assistant", "content": [{
                    "type": "tool_use", "id": "call-1", "name": tool_name, "input": {}
                }]
            }
        });
        let result = serde_json::json!({
            "type": "user", "session_id": "s", "message": {
                "role": "user", "content": [{
                    "type": "tool_result", "tool_use_id": "call-1", "content": "ok",
                    "is_error": result_error
                }]
            }
        });
        let mut transcript = vec![
            claudecode::RawEvent::from_value(init).unwrap(),
            claudecode::RawEvent::from_value(call).unwrap(),
        ];
        if include_result {
            transcript.push(claudecode::RawEvent::from_value(result).unwrap());
        }
        SessionOutcome {
            result: claudecode::ClaudeResult {
                result: Some("final".to_string()),
                content: Some("final".to_string()),
                ..Default::default()
            },
            transcript,
            exit_code: Some(0),
            raw_stdout: String::new(),
            stderr: String::new(),
            invocation: claudecode::InvocationMetadata::default(),
        }
    }

    #[test]
    fn accepts_paired_successful_non_todo_evidence() {
        let tool = "mcp__agentic-mcp__cli_ls".to_string();
        assert_eq!(
            validate_outcome(&outcome(&tool, true, false), std::slice::from_ref(&tool)).unwrap(),
            "final"
        );
    }

    #[test]
    fn rejects_unpaired_errored_todo_and_disallowed_evidence() {
        let read = "mcp__agentic-mcp__workspace_read".to_string();
        assert!(
            validate_outcome(&outcome(&read, false, false), std::slice::from_ref(&read)).is_err()
        );
        assert!(
            validate_outcome(&outcome(&read, true, true), std::slice::from_ref(&read)).is_err()
        );

        let todo = "mcp__agentic-mcp__workspace_todowrite".to_string();
        assert!(
            validate_outcome(&outcome(&todo, true, false), std::slice::from_ref(&todo)).is_err()
        );
        assert!(
            validate_outcome(&outcome(&read, true, false), std::slice::from_ref(&todo)).is_err()
        );
    }

    #[test]
    fn rejects_prose_only_and_init_mismatch() {
        let tool = "mcp__agentic-mcp__cli_ls".to_string();
        let mut prose = outcome(&tool, false, false);
        prose.transcript.truncate(1);
        assert!(validate_outcome(&prose, std::slice::from_ref(&tool)).is_err());
        assert!(validate_outcome(&outcome(&tool, true, false), &["other".to_string()]).is_err());
    }

    #[test]
    fn rejects_terminal_init_and_out_of_order_failures() {
        let tool = "mcp__agentic-mcp__cli_ls".to_string();

        let mut nonzero = outcome(&tool, true, false);
        nonzero.exit_code = Some(9);
        assert!(validate_outcome(&nonzero, std::slice::from_ref(&tool)).is_err());

        let mut terminal = outcome(&tool, true, false);
        terminal.result.is_error = true;
        assert!(validate_outcome(&terminal, std::slice::from_ref(&tool)).is_err());

        let mut init_error = outcome(&tool, true, false);
        init_error.transcript[0] = claudecode::RawEvent::from_value(serde_json::json!({
            "type": "system", "subtype": "init", "session_id": "s",
            "tools": [tool], "mcp_server_errors": [{"error": "failed"}]
        }))
        .unwrap();
        assert!(validate_outcome(&init_error, std::slice::from_ref(&tool)).is_err());

        let mut reversed = outcome(&tool, true, false);
        reversed.transcript.swap(1, 2);
        assert!(validate_outcome(&reversed, std::slice::from_ref(&tool)).is_err());
    }

    #[test]
    fn rejects_wrong_role_tool_blocks() {
        let tool = "mcp__agentic-mcp__cli_ls".to_string();
        let mut wrong_use = outcome(&tool, true, false);
        wrong_use.transcript[1] = claudecode::RawEvent::from_value(serde_json::json!({
            "type": "user", "session_id": "s", "message": {
                "role": "user", "content": [{
                    "type": "tool_use", "id": "call-1", "name": tool, "input": {}
                }]
            }
        }))
        .unwrap();
        assert!(validate_outcome(&wrong_use, std::slice::from_ref(&tool)).is_err());

        let mut wrong_result = outcome(&tool, true, false);
        wrong_result.transcript[2] = claudecode::RawEvent::from_value(serde_json::json!({
            "type": "assistant", "session_id": "s", "message": {
                "role": "assistant", "content": [{
                    "type": "tool_result", "tool_use_id": "call-1", "content": "ok",
                    "is_error": false
                }]
            }
        }))
        .unwrap();
        assert!(validate_outcome(&wrong_result, std::slice::from_ref(&tool)).is_err());
    }

    #[test]
    fn rejects_leftover_unpaired_tool_use_after_valid_evidence() {
        let tool = "mcp__agentic-mcp__cli_ls".to_string();
        let mut partial = outcome(&tool, true, false);
        partial.transcript.push(
            claudecode::RawEvent::from_value(serde_json::json!({
                "type": "assistant", "session_id": "s", "message": {
                    "role": "assistant", "content": [{
                        "type": "tool_use", "id": "call-2", "name": tool, "input": {}
                    }]
                }
            }))
            .unwrap(),
        );
        assert!(validate_outcome(&partial, std::slice::from_ref(&tool)).is_err());
    }

    #[test]
    fn nonce_requires_successful_paired_expected_tool_result_and_prompt_absence() {
        let tool = "mcp__agentic-mcp__cli_ls".to_string();
        let valid = outcome(&tool, true, false);
        let mut valid = valid;
        valid.transcript[2] = claudecode::RawEvent::from_value(serde_json::json!({
            "type": "user", "session_id": "s", "message": {
                "role": "user", "content": [{
                    "type": "tool_result", "tool_use_id": "call-1", "content": "nonce-123",
                    "is_error": false
                }]
            }
        }))
        .unwrap();
        assert!(
            validate_nonce_provenance(
                &valid,
                std::slice::from_ref(&tool),
                &["find marker", "system guidance"],
                "nonce-123"
            )
            .is_ok()
        );
        assert!(
            validate_nonce_provenance(
                &valid,
                std::slice::from_ref(&tool),
                &["find marker", "system guidance includes nonce-123"],
                "nonce-123"
            )
            .is_err()
        );

        let mut prose = valid.clone();
        prose.transcript[2] = claudecode::RawEvent::from_value(serde_json::json!({
            "type": "user", "session_id": "s", "message": {
                "role": "user", "content": [{"type": "text", "text": "nonce-123"}]
            }
        }))
        .unwrap();
        assert!(
            validate_nonce_provenance(
                &prose,
                std::slice::from_ref(&tool),
                &["find marker"],
                "nonce-123"
            )
            .is_err()
        );

        let errored = outcome(&tool, true, true);
        assert!(
            validate_nonce_provenance(
                &errored,
                std::slice::from_ref(&tool),
                &["find marker"],
                "ok"
            )
            .is_err()
        );

        let other = "mcp__agentic-mcp__cli_grep".to_string();
        assert!(
            validate_nonce_provenance(
                &valid,
                std::slice::from_ref(&other),
                &["find marker"],
                "nonce-123"
            )
            .is_err()
        );

        let mut duplicate = valid.clone();
        duplicate.transcript.insert(
            2,
            claudecode::RawEvent::from_value(serde_json::json!({
                "type": "assistant", "session_id": "s", "message": {
                    "role": "assistant", "content": [{
                        "type": "tool_use", "id": "call-1", "name": other, "input": {}
                    }]
                }
            }))
            .unwrap(),
        );
        assert!(
            validate_nonce_provenance(
                &duplicate,
                std::slice::from_ref(&tool),
                &["find marker"],
                "nonce-123"
            )
            .is_err()
        );

        let mut wrong_use = valid.clone();
        wrong_use.transcript[1] = claudecode::RawEvent::from_value(serde_json::json!({
            "type": "user", "session_id": "s", "message": {
                "role": "user", "content": [{
                    "type": "tool_use", "id": "call-1", "name": tool, "input": {}
                }]
            }
        }))
        .unwrap();
        assert!(
            validate_nonce_provenance(
                &wrong_use,
                std::slice::from_ref(&tool),
                &["find marker"],
                "nonce-123"
            )
            .is_err()
        );

        let mut wrong_result = valid.clone();
        wrong_result.transcript[2] = claudecode::RawEvent::from_value(serde_json::json!({
            "type": "assistant", "session_id": "s", "message": {
                "role": "assistant", "content": [{
                    "type": "tool_result", "tool_use_id": "call-1", "content": "nonce-123",
                    "is_error": false
                }]
            }
        }))
        .unwrap();
        assert!(
            validate_nonce_provenance(
                &wrong_result,
                std::slice::from_ref(&tool),
                &["find marker"],
                "nonce-123"
            )
            .is_err()
        );

        let mut leftover = valid.clone();
        leftover.transcript.push(
            claudecode::RawEvent::from_value(serde_json::json!({
                "type": "assistant", "session_id": "s", "message": {
                    "role": "assistant", "content": [{
                        "type": "tool_use", "id": "call-2", "name": tool, "input": {}
                    }]
                }
            }))
            .unwrap(),
        );
        assert!(
            validate_nonce_provenance(
                &leftover,
                std::slice::from_ref(&tool),
                &["find marker"],
                "nonce-123"
            )
            .is_err()
        );
    }
}
