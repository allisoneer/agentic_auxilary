use crate::engine::{
    config::select_optimizer_model,
    directory::expand_directories_to_filemeta,
    guards::{ensure_plan_template_group, maybe_inject_plan_structure_meta},
    memory::{auto_inject_claude_memories, injection_enabled_from_env},
    paths::{dedup_files_in_place, normalize_paths_in_place, precheck_files},
};
use crate::optimizer::parser::OptimizerOutput;
use crate::{
    client::OrClient,
    errors::*,
    optimizer::{call_optimizer, parser::parse_optimizer_output},
    template::inject_files,
    token::enforce_limit,
    types::{DirectoryMeta, FileMeta, PromptType},
};
use agentic_logging::{CallTimer, LogWriter, ToolCallRecord};
use async_openai::types::*;
use thoughts_tool::{DocumentType, write_document};
use universal_tool_core::prelude::ToolError;

pub async fn gpt5_reasoner_impl(
    prompt: String,
    mut files: Vec<FileMeta>,
    directories: Option<Vec<DirectoryMeta>>,
    optimizer_model: Option<String>,
    prompt_type: PromptType,
    output_filename: Option<String>,
) -> std::result::Result<String, ToolError> {
    // Start logging timer
    let timer = CallTimer::start();
    let server = "gpt5_reasoner".to_string();
    let tool = match prompt_type {
        PromptType::Plan => "plan",
        PromptType::Reasoning => "reasoning",
    }
    .to_string();

    // Initialize writer early (best-effort)
    let writer = thoughts_tool::active_logs_dir().ok().map(LogWriter::new);

    // Best-effort JSONL append closure
    // Takes files_count as parameter to avoid capturing `files` (which is mutated later)
    let log_record = |success: bool,
                      error: Option<String>,
                      response_file: Option<String>,
                      model: Option<String>,
                      token_usage: Option<agentic_logging::TokenUsage>,
                      files_count: usize| {
        if let Some(ref w) = writer {
            let (completed_at, duration_ms) = timer.finish();
            // TODO(2): Consider truncating large payloads (prompt, directories) to reduce log
            // bloat. Tradeoff: full content is valuable for debugging failures.
            let request_json = serde_json::json!({
                "prompt_type": format!("{:?}", prompt_type).to_lowercase(),
                "prompt": prompt,
                "directories": directories,
                "files_count": files_count,
                "output_filename": output_filename,
            });
            let record = ToolCallRecord {
                call_id: timer.call_id.clone(),
                server: server.clone(),
                tool: tool.clone(),
                started_at: timer.started_at,
                completed_at,
                duration_ms,
                request: request_json,
                response_file,
                success,
                error,
                model,
                token_usage,
                summary: None,
            };
            if let Err(e) = w.append_jsonl(&record) {
                tracing::warn!("Failed to append JSONL log: {}", e);
            }
        }
    };

    // Expand directories to files BEFORE optimizer sees them
    if let Some(dirs) = directories.as_ref() {
        let expanded = match expand_directories_to_filemeta(dirs) {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("stage=expand_directories: {}", e);
                log_record(false, Some(msg), None, None, None, files.len());
                return Err(ToolError::from(e));
            }
        };
        files.extend(expanded);
    }

    // ===== NEW: normalize + validate BEFORE optimizer =====
    tracing::debug!("Normalizing {} file path(s) to absolute", files.len());
    normalize_paths_in_place(&mut files);

    let before = files.len();
    dedup_files_in_place(&mut files);
    let after = files.len();
    if before != after {
        tracing::debug!(
            "Deduplicated files post-normalization: {} -> {} ({} removed)",
            before,
            after,
            before - after
        );
    }

    // NEW: CLAUDE.md auto-injection (default-on; can be disabled by env)
    if injection_enabled_from_env() {
        let injected = auto_inject_claude_memories(&mut files, directories.as_deref());
        if injected > 0 {
            // Dedup again in case user passed CLAUDE.md explicitly
            let before = files.len();
            dedup_files_in_place(&mut files);
            let after = files.len();
            if before != after {
                tracing::debug!(
                    "Deduplicated files after CLAUDE.md injection: {} -> {} ({} removed)",
                    before,
                    after,
                    before - after
                );
            }
        }
    } else {
        tracing::info!("CLAUDE.md auto-injection disabled via INJECT_CLAUDE_MD");
    }

    // Pre-validate after injection so newly discovered files are checked
    tracing::info!("Pre-validating {} file(s) before optimizer", files.len());
    if let Err(e) = precheck_files(&files) {
        let msg = format!("stage=precheck_files: {}", e);
        log_record(false, Some(msg), None, None, None, files.len());
        return Err(e);
    }
    // ===== END NEW =====

    // Auto-inject plan_structure.md for Plan prompts (before optimizer)
    maybe_inject_plan_structure_meta(&prompt_type, &mut files);

    // Load env OpenRouter key (CLI already optionally did dotenv)
    let client = OrClient::from_env().map_err(ToolError::from)?;

    // Step 1: optimize with retry on validation errors
    let opt_model = select_optimizer_model(optimizer_model);

    // Layer 3: Validation retry (complements Layer 2 network retry in optimizer/mod.rs)
    const TEMPLATE_RETRIES: usize = 2; // 3 total attempts
    const TEMPLATE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(900);

    let mut parsed: Option<OptimizerOutput> = None;

    for attempt in 0..=TEMPLATE_RETRIES {
        if attempt > 0 {
            tracing::warn!(
                "Retrying optimizer due to template validation error (attempt {} of {})",
                attempt + 1,
                TEMPLATE_RETRIES + 1
            );
            tokio::time::sleep(TEMPLATE_RETRY_DELAY).await;
        }

        // Call optimizer (this has its own Layer 2 network retry)
        let raw = match call_optimizer(&client, &opt_model, &prompt_type, &prompt, &files).await {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("stage=optimizer_call: {}", e);
                log_record(false, Some(msg), None, None, None, files.len());
                return Err(ToolError::from(e));
            }
        };

        tracing::debug!("Raw optimizer output:\n{}", raw);

        // Parse and validate
        match parse_optimizer_output(&raw) {
            Ok(p) => {
                parsed = Some(p);
                break; // Success - exit retry loop
            }
            Err(e) => {
                // Only retry Template validation errors, not other parse errors
                let is_template_error = matches!(e, ReasonerError::Template(_));

                if is_template_error && attempt < TEMPLATE_RETRIES {
                    tracing::warn!("Template validation failed: {}; retrying optimizer call", e);
                    continue;
                }

                // Final failure or non-retryable error
                let stage = if is_template_error {
                    tracing::error!(
                        "Template validation failed after {} attempts. Raw output (first 800 chars):\n{}",
                        attempt + 1,
                        raw.chars().take(800).collect::<String>()
                    );
                    "template_validation_exhausted"
                } else {
                    tracing::error!("Non-template parse error: {}", e);
                    "parse_output"
                };

                let msg = format!("stage={}: {}", stage, e);
                log_record(false, Some(msg), None, None, None, files.len());
                return Err(ToolError::from(e));
            }
        }
    }

    let mut parsed = parsed.expect("retry loop must exit via break or return");

    tracing::debug!(
        "Parsed optimizer output: {} groups found",
        parsed.groups.file_groups.len()
    );
    for group in &parsed.groups.file_groups {
        tracing::debug!("  Group '{}': {} files", group.name, group.files.len());
    }

    // Executor-side guard: ensure plan_template group and safe marker
    if matches!(prompt_type, PromptType::Plan) {
        ensure_plan_template_group(&mut parsed);
    }

    // Step 2: inject, token check, execute
    let mut final_prompt = match inject_files(&parsed.xml_template, &parsed.groups).await {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("stage=inject_files: {}", e);
            log_record(false, Some(msg), None, None, None, files.len());
            return Err(ToolError::from(e));
        }
    };

    // Replace the {original_prompt} placeholder with the actual prompt
    final_prompt = final_prompt.replace("{original_prompt}", &prompt);

    let token_count = match crate::token::count_tokens(&final_prompt) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("stage=count_tokens: {}", e);
            log_record(false, Some(msg), None, None, None, files.len());
            return Err(ToolError::from(e));
        }
    };
    tracing::debug!("Final prompt token count: {}", token_count);
    tracing::debug!(
        "Final prompt after injection (first 500 chars):\n{}...",
        final_prompt.chars().take(500).collect::<String>()
    );

    if let Err(e) = enforce_limit(&final_prompt) {
        let msg = format!("stage=enforce_limit: {}", e);
        log_record(false, Some(msg), None, None, None, files.len());
        return Err(ToolError::from(e));
    }

    // Execute GPT-5 with application-level retries for network/transport errors
    const GPT5_RETRIES: usize = 1;
    const GPT5_DELAY: std::time::Duration = std::time::Duration::from_millis(750);

    tracing::debug!("Executing final prompt with openai/gpt-5 at high reasoning effort");

    for attempt in 0..=GPT5_RETRIES {
        if attempt > 0 {
            tracing::warn!("GPT-5 API attempt {} of {}", attempt + 1, GPT5_RETRIES + 1);
            tokio::time::sleep(GPT5_DELAY).await;
        }

        // Build request inside the loop; clone final_prompt to keep ownership
        let user_msg = match ChatCompletionRequestUserMessageArgs::default()
            .content(final_prompt.clone())
            .build()
        {
            Ok(m) => m,
            Err(e) => {
                let msg = format!("stage=build_chat_request: user message build failed: {}", e);
                log_record(false, Some(msg), None, None, None, files.len());
                return Err(ToolError::from(ReasonerError::from(e)));
            }
        };

        let req = match CreateChatCompletionRequestArgs::default()
            .model("openai/gpt-5")
            .messages([ChatCompletionRequestMessage::User(user_msg)])
            .reasoning_effort(ReasoningEffort::High)
            .temperature(0.2)
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("stage=build_chat_request: request build failed: {}", e);
                log_record(false, Some(msg), None, None, None, files.len());
                return Err(ToolError::from(ReasonerError::from(e)));
            }
        };

        let start = std::time::Instant::now();
        match client.client.chat().create(req).await {
            Ok(resp) => {
                let duration = start.elapsed();
                tracing::debug!("GPT-5 API succeeded in {:?}", duration);

                // NEW: log response metadata + classify emptiness
                let empty_kind = crate::logging::log_chat_response("executor", &resp, duration);

                // Extract content option
                let content_opt = resp.choices.first().and_then(|c| c.message.content.clone());

                // Determine if content is effectively empty
                let is_effectively_empty = match &content_opt {
                    None => true,
                    Some(s) if s.is_empty() => true,
                    Some(s) if s.trim().is_empty() => true,
                    _ => false,
                };

                if is_effectively_empty {
                    // Warn with specific classification if available
                    if let Some(kind) = empty_kind {
                        crate::logging::log_empty_warning("executor", kind, &resp);
                    } else {
                        tracing::warn!("Executor received empty content (unclassified)");
                    }

                    // NEW: Treat as retryable anomaly once, then return helpful error
                    if attempt < GPT5_RETRIES {
                        tracing::warn!(
                            "Empty response from GPT-5; retrying once (attempt {} of {})",
                            attempt + 2,
                            GPT5_RETRIES + 1
                        );
                        continue;
                    }

                    // Final failure after retry
                    let err_msg = format!(
                        "Reasoning model returned no response after {} attempt(s). \
                         Possible causes: content filtering, prompt issues, or API anomaly.",
                        attempt + 1
                    );
                    tracing::error!("{}", err_msg);
                    log_record(
                        false,
                        Some(format!("stage=empty_response: {}", err_msg)),
                        None,
                        Some("openai/gpt-5".to_string()),
                        None,
                        files.len(),
                    );
                    return Err(ToolError::new(
                        universal_tool_core::error::ErrorCode::ExecutionFailed,
                        "Reasoning model returned no response after retry. \
                         Check logs for response metadata (id, finish_reason, usage). \
                         Possible causes: content filtering, prompt issues, or API anomaly.",
                    ));
                }

                // Non-empty content → success
                let content = content_opt.expect("guarded by is_effectively_empty=false");

                // Extract token usage (includes reasoning_tokens when present)
                let usage = crate::logging::extract_token_usage(&resp);

                let mut response_file = None;
                let returned: String;

                match prompt_type {
                    PromptType::Plan => {
                        if let Some(ref name) = output_filename {
                            // Write plan to plans/ directory
                            match write_document(DocumentType::Plan, name, &content) {
                                Ok(ok) => {
                                    returned = ok.path.clone();
                                }
                                Err(e) => {
                                    let msg = format!("stage=write_plan_document: {}", e);
                                    log_record(
                                        false,
                                        Some(msg),
                                        None,
                                        Some("openai/gpt-5".to_string()),
                                        None,
                                        files.len(),
                                    );
                                    return Err(ToolError::internal(e.to_string()));
                                }
                            }
                        } else {
                            // Return content directly
                            returned = content.clone();
                        }
                    }
                    PromptType::Reasoning => {
                        // Write response markdown to logs/ daily dir (best-effort)
                        if let Some(ref w) = writer {
                            let (completed_at, _) = timer.finish();
                            if let Ok(md_name) =
                                w.write_markdown_response(completed_at, &timer.call_id, &content)
                                && !md_name.is_empty()
                            {
                                response_file = Some(md_name);
                            }
                        }
                        returned = content.clone();
                    }
                }

                // Append JSONL via closure (best-effort)
                log_record(
                    true,
                    None,
                    response_file,
                    Some("openai/gpt-5".to_string()),
                    usage,
                    files.len(),
                );

                return Ok(returned);
            }
            Err(e) => {
                let retryable = crate::errors::is_retryable_app_level(&e);
                if attempt < GPT5_RETRIES && retryable {
                    tracing::warn!("GPT-5 call failed with retryable error: {e}; retrying...");
                    continue;
                }

                // Not retryable or retries exhausted
                if retryable {
                    tracing::error!(
                        "GPT-5 call failed after {} attempts with retryable error: {}",
                        attempt + 1,
                        e
                    );
                } else {
                    tracing::error!("GPT-5 call failed with non-retryable error: {}", e);
                }
                let msg = format!("stage=chat_execute: {}", e);
                log_record(
                    false,
                    Some(msg),
                    None,
                    Some("openai/gpt-5".to_string()),
                    None,
                    files.len(),
                );
                return Err(ToolError::from(ReasonerError::from(e)));
            }
        }
    }

    // Should never reach here due to loop logic, but provide a defensive error
    log_record(
        false,
        Some("stage=final_unreachable: GPT-5 failed after all retries".to_string()),
        None,
        Some("openai/gpt-5".to_string()),
        None,
        files.len(),
    );
    Err(ToolError::new(
        universal_tool_core::error::ErrorCode::ExecutionFailed,
        "GPT-5 failed after all retries",
    ))
}

#[cfg(test)]
mod retry_tests {
    use crate::errors::ReasonerError;

    #[test]
    fn test_template_error_is_retryable() {
        let template_err = ReasonerError::Template("missing marker".into());
        assert!(matches!(template_err, ReasonerError::Template(_)));
    }

    #[test]
    fn test_yaml_error_is_not_template_error() {
        // Create a YAML error by parsing invalid YAML
        let yaml_result: Result<serde_yaml::Value, _> =
            serde_yaml::from_str("invalid: yaml: syntax");
        assert!(yaml_result.is_err());

        let yaml_err = ReasonerError::Yaml(yaml_result.unwrap_err());
        assert!(!matches!(yaml_err, ReasonerError::Template(_)));
    }
}
