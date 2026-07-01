use crate::client::OrClient;
use crate::engine::directory::expand_directories_to_filemeta;
use crate::engine::guards::ensure_plan_template_group;
use crate::engine::guards::maybe_inject_plan_structure_meta;
use crate::engine::memory::auto_inject_claude_memories;
use crate::engine::memory::injection_enabled_from_env;
use crate::engine::paths::dedup_files_in_place;
use crate::engine::paths::normalize_paths_in_place;
use crate::engine::paths::precheck_files;
use crate::engine::preflight;
use crate::errors::ReasonerError;
use crate::optimizer::call_optimizer;
use crate::optimizer::parser::OptimizerOutput;
use crate::optimizer::parser::parse_optimizer_output;
use crate::template::inject_files;
use crate::token::enforce_limit;
use crate::types::DirectoryMeta;
use crate::types::FileMeta;
use crate::types::PromptType;
use agentic_config::types::ReasoningConfig;
use agentic_logging::CallTimer;
use agentic_logging::LogWriter;
use agentic_logging::ToolCallRecord;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use async_openai::error::OpenAIError;
use async_openai::types::chat::ChatCompletionRequestMessage;
use async_openai::types::chat::ChatCompletionRequestUserMessageArgs;
use async_openai::types::chat::ChatCompletionStreamOptions;
use async_openai::types::chat::CompletionUsage;
use async_openai::types::chat::CreateChatCompletionRequestArgs;
use async_openai::types::chat::FinishReason;
use async_openai::types::chat::ReasoningEffort;
use futures::StreamExt;
use std::collections::HashSet;
use thoughts_tool::DocumentType;
use thoughts_tool::write_document;

const PARTIAL_REASONING_MARKER: &str = "> **Warning:** Partial response (executor stream interrupted). Content below may be incomplete.\n\n";

const PARTIAL_PLAN_MARKER: &str = "**WARNING: INCOMPLETE PLAN**\nThe plan below may be incomplete because the executor stream ended unexpectedly.\n\n---\n\n";

const TEMPLATE_RETRIES: usize = 2;
const TEMPLATE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(900);
const EXECUTOR_RETRIES: usize = 1;
const EXECUTOR_DELAY: std::time::Duration = std::time::Duration::from_millis(750);

#[derive(Debug)]
enum ExecutorStreamError {
    Cancelled,
    OpenAI(OpenAIError),
}

#[derive(Debug, Default)]
struct ExecutorStreamState {
    content: String,
    usage: Option<CompletionUsage>,
    chunks: usize,
    first_content_ms: Option<u128>,
    response_id: Option<String>,
    finish_reason: Option<FinishReason>,
}

#[derive(Clone, Copy)]
struct ExecutorStreamOutcome<'a> {
    partial: bool,
    timeout: bool,
    empty: bool,
    stream_error: Option<&'a str>,
    stream_error_class: Option<&'a str>,
}

impl ExecutorStreamState {
    fn has_content(&self) -> bool {
        !self.content.trim().is_empty()
    }
}

fn prepend_partial_output(prompt_type: &PromptType, content: &str) -> String {
    match prompt_type {
        PromptType::Reasoning => format!("{PARTIAL_REASONING_MARKER}{content}"),
        PromptType::Plan => format!("{PARTIAL_PLAN_MARKER}{content}"),
    }
}

fn executor_stream_summary(
    attempt: usize,
    duration: std::time::Duration,
    state: &ExecutorStreamState,
    outcome: ExecutorStreamOutcome<'_>,
) -> serde_json::Value {
    let completion_tokens_details = state
        .usage
        .as_ref()
        .and_then(|usage| usage.completion_tokens_details.clone());

    serde_json::json!({
        "attempt_index": attempt,
        "attempt_duration_ms": duration.as_millis(),
        "chunks_received": state.chunks,
        "chars": state.content.len(),
        "time_to_first_content_ms": state.first_content_ms,
        "usage_chunk_observed": state.usage.is_some(),
        "partial": outcome.partial,
        "timeout": outcome.timeout,
        "empty": outcome.empty,
        "stream_error": outcome.stream_error,
        "response_id": state.response_id.clone(),
        "finish_reason": state.finish_reason.clone(),
        "completion_tokens_details": completion_tokens_details,
        "stream_error_class": outcome.stream_error_class,
    })
}

fn stream_error_class_from_openai(e: &OpenAIError) -> &'static str {
    match e {
        OpenAIError::Reqwest(_) => "reqwest",
        OpenAIError::StreamError(_) => "stream",
        OpenAIError::JSONDeserialize(_, _) => "json_deserialize",
        OpenAIError::ApiError(_) => "api",
        OpenAIError::InvalidArgument(_) => "invalid_argument",
        _ => "other",
    }
}

/// Parse reasoning effort string to enum, defaulting to Xhigh.
fn parse_reasoning_effort(v: Option<&str>) -> ReasoningEffort {
    match v.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("low") => ReasoningEffort::Low,
        Some("medium") => ReasoningEffort::Medium,
        Some("high") => ReasoningEffort::High,
        _ => ReasoningEffort::Xhigh, // default
    }
}

pub async fn gpt5_reasoner_impl(
    prompt: String,
    mut files: Vec<FileMeta>,
    directories: Option<Vec<DirectoryMeta>>,
    cfg: &ReasoningConfig,
    prompt_type: PromptType,
    output_filename: Option<String>,
    ctx: &ToolContext,
) -> std::result::Result<String, ToolError> {
    if ctx.is_cancelled() {
        return Err(ToolError::cancelled(None));
    }

    // Start logging timer
    let timer = CallTimer::start();
    let server = "gpt5_reasoner".to_string();
    let tool = match prompt_type {
        PromptType::Plan => "plan",
        PromptType::Reasoning => "reasoning",
    }
    .to_string();

    // Initialize writer early (best-effort)
    let writer = match thoughts_tool::active_logs_dir() {
        Ok(dir) => Some(LogWriter::new(dir)),
        Err(e) => {
            tracing::debug!("JSONL logging unavailable: {}", e);
            None
        }
    };

    // Best-effort JSONL append closure
    // Takes files_count as parameter to avoid capturing `files` (which is mutated later)
    let log_record = |success: bool,
                      error: Option<String>,
                      response_file: Option<String>,
                      model: Option<String>,
                      token_usage: Option<agentic_logging::TokenUsage>,
                      files_count: usize,
                      summary: Option<serde_json::Value>| {
        if let Some(ref w) = writer {
            let (completed_at, duration_ms) = timer.finish();
            // TODO(2): Consider truncating large payloads (prompt, directories) to reduce log
            // bloat. Tradeoff: full content is valuable for debugging failures.
            let request_json = serde_json::json!({
                "prompt_type": tool,
                "prompt": prompt,
                "directories": directories,
                "files_count": files_count,
                "output_filename": output_filename,
            });
            let failure_kind = agentic_logging::classify_failure_kind(success, error.as_deref());
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
                failure_kind,
                model,
                token_usage,
                summary,
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
                let msg = format!("stage=expand_directories: {e}");
                log_record(false, Some(msg), None, None, None, files.len(), None);
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

    // Auto-inject plan_structure.md for Plan prompts before preflight and optimizer.
    maybe_inject_plan_structure_meta(&prompt_type, &mut files);

    let aggregate_stats = match preflight::aggregate_corpus_preflight(&prompt_type, &prompt, &files)
    {
        Ok(stats) => stats,
        Err(e) => {
            let msg = format!("stage=preflight_aggregate: {e}");
            log_record(false, Some(msg), None, None, None, files.len(), None);
            return Err(ToolError::InvalidInput(format!(
                "stage=preflight_aggregate: {e}"
            )));
        }
    };
    tracing::info!(
        unique_files = aggregate_stats.unique_files,
        fs_bytes = aggregate_stats.fs_bytes,
        optimizer_prompt_tokens_est = aggregate_stats.optimizer_prompt_tokens_est,
        "Aggregate corpus preflight passed"
    );
    let allowed_paths: HashSet<String> = files.iter().map(|f| f.filename.clone()).collect();

    // Pre-validate after aggregate preflight so newly discovered files are checked.
    tracing::info!("Pre-validating {} file(s) before optimizer", files.len());
    if let Err(e) = precheck_files(&files) {
        let msg = format!("stage=precheck_files: {e}");
        log_record(false, Some(msg), None, None, None, files.len(), None);
        return Err(e);
    }
    // ===== END NEW =====

    if ctx.is_cancelled() {
        return Err(ToolError::cancelled(None));
    }

    // Load env OpenRouter key (CLI already optionally did dotenv)
    let client = OrClient::from_env(cfg.api_base_url.as_deref()).map_err(ToolError::from)?;

    // Step 1: optimize with retry on validation errors
    let opt_model = cfg.optimizer_model.clone();

    // Layer 3: Validation retry (complements Layer 2 network retry in optimizer/mod.rs)

    let mut parsed: Option<OptimizerOutput> = None;

    for attempt in 0..=TEMPLATE_RETRIES {
        if attempt > 0 {
            tracing::warn!(
                "Retrying optimizer due to template validation error (attempt {} of {})",
                attempt + 1,
                TEMPLATE_RETRIES + 1
            );
            tokio::select! {
                () = ctx.cancelled() => return Err(ToolError::cancelled(None)),
                () = tokio::time::sleep(TEMPLATE_RETRY_DELAY) => {}
            }
        }

        // Call optimizer (this has its own Layer 2 network retry)
        let raw = match ctx
            .run_cancellable(call_optimizer(
                &client,
                &opt_model,
                &prompt_type,
                &prompt,
                &files,
            ))
            .await
        {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("stage=optimizer_call: {e}");
                log_record(false, Some(msg), None, None, None, files.len(), None);
                return Err(e);
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

                let msg = format!("stage={stage}: {e}");
                log_record(false, Some(msg), None, None, None, files.len(), None);
                return Err(ToolError::from(e));
            }
        }
    }

    let Some(mut parsed) = parsed else {
        let msg = "stage=template_validation: optimizer retry loop exited without a result";
        log_record(
            false,
            Some(msg.to_string()),
            None,
            None,
            None,
            files.len(),
            None,
        );
        return Err(ToolError::internal(msg));
    };

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

    if let Err(e) = preflight::selected_file_subset_preflight(&allowed_paths, &parsed.groups) {
        let msg = format!("stage=preflight_selected_files: {e}");
        log_record(
            false,
            Some(msg.clone()),
            None,
            None,
            None,
            files.len(),
            None,
        );
        return Err(ToolError::InvalidInput(msg));
    }

    // Step 2: inject, token check, execute
    let mut final_prompt = match inject_files(&parsed.xml_template, &parsed.groups).await {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("stage=inject_files: {e}");
            log_record(false, Some(msg), None, None, None, files.len(), None);
            return Err(ToolError::from(e));
        }
    };

    // Replace the {original_prompt} placeholder with the actual prompt
    final_prompt = final_prompt.replace("{original_prompt}", &prompt);

    let token_count = match crate::token::count_tokens(&final_prompt) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("stage=count_tokens: {e}");
            log_record(false, Some(msg), None, None, None, files.len(), None);
            return Err(ToolError::from(e));
        }
    };
    tracing::debug!("Final prompt token count: {}", token_count);
    tracing::debug!(
        "Final prompt after injection (first 500 chars):\n{}...",
        final_prompt.chars().take(500).collect::<String>()
    );

    if let Err(e) = enforce_limit(&final_prompt, cfg.max_input_tokens) {
        let msg = format!("stage=enforce_limit: {e}");
        log_record(false, Some(msg), None, None, None, files.len(), None);
        return Err(ToolError::from(e));
    }

    let finalize_executor_success = |content: String,
                                     usage: Option<agentic_logging::TokenUsage>,
                                     partial: bool,
                                     summary: Option<serde_json::Value>|
     -> std::result::Result<String, ToolError> {
        let rendered = if partial {
            prepend_partial_output(&prompt_type, &content)
        } else {
            content
        };

        let mut response_file = None;
        let returned: String;

        match prompt_type {
            PromptType::Plan => {
                if let Some(ref name) = output_filename {
                    match write_document(&DocumentType::Plan, name, &rendered) {
                        Ok(ok) => {
                            returned = ok.path;
                        }
                        Err(e) => {
                            let msg = format!("stage=write_plan_document: {e}");
                            log_record(
                                false,
                                Some(msg),
                                None,
                                Some(cfg.executor_model.clone()),
                                None,
                                files.len(),
                                summary,
                            );
                            return Err(ToolError::internal(e.to_string()));
                        }
                    }
                } else {
                    returned = rendered;
                }
            }
            PromptType::Reasoning => {
                if let Some(ref w) = writer {
                    let (completed_at, _) = timer.finish();
                    if let Ok(md_name) =
                        w.write_markdown_response(completed_at, &timer.call_id, &rendered)
                        && !md_name.is_empty()
                    {
                        response_file = Some(md_name);
                    }
                }
                returned = rendered;
            }
        }

        log_record(
            true,
            None,
            response_file,
            Some(cfg.executor_model.clone()),
            usage,
            files.len(),
            summary,
        );

        Ok(returned)
    };

    // Execute with application-level retries for network/transport errors

    let executor_model = cfg.executor_model.as_str();
    let reasoning_effort = parse_reasoning_effort(cfg.reasoning_effort.as_deref());

    tracing::debug!(
        "Executing final prompt with {} at {:?} reasoning effort",
        executor_model,
        reasoning_effort
    );

    for attempt in 0..=EXECUTOR_RETRIES {
        if attempt > 0 {
            tracing::warn!(
                "Executor API attempt {} of {}",
                attempt + 1,
                EXECUTOR_RETRIES + 1
            );
            tokio::select! {
                () = ctx.cancelled() => return Err(ToolError::cancelled(None)),
                () = tokio::time::sleep(EXECUTOR_DELAY) => {}
            }
        }

        // Build request inside the loop; clone final_prompt to keep ownership
        let user_msg = match ChatCompletionRequestUserMessageArgs::default()
            .content(final_prompt.clone())
            .build()
        {
            Ok(m) => m,
            Err(e) => {
                let msg = format!("stage=build_chat_request: user message build failed: {e}");
                log_record(false, Some(msg), None, None, None, files.len(), None);
                return Err(ToolError::from(ReasonerError::from(e)));
            }
        };

        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder
            .model(executor_model)
            .messages([ChatCompletionRequestMessage::User(user_msg)])
            .reasoning_effort(reasoning_effort.clone())
            .temperature(0.2)
            .stream_options(ChatCompletionStreamOptions {
                include_usage: Some(true),
                include_obfuscation: None,
            });
        if let Some(n) = cfg.max_completion_tokens {
            req_builder.max_completion_tokens(n);
        }

        let req = match req_builder.build() {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("stage=build_chat_request: request build failed: {e}");
                log_record(false, Some(msg), None, None, None, files.len(), None);
                return Err(ToolError::from(ReasonerError::from(e)));
            }
        };

        let attempt_started = std::time::Instant::now();
        let executor_timeout = std::time::Duration::from_secs(cfg.executor_timeout_secs);
        let heartbeat = std::time::Duration::from_secs(cfg.stream_heartbeat_secs);
        let mut stream_state = ExecutorStreamState::default();

        let stream_result = tokio::time::timeout(executor_timeout, async {
            let chat = client.client.chat();
            let mut stream = tokio::select! {
                () = ctx.cancelled() => return Err(ExecutorStreamError::Cancelled),
                response = chat.create_stream(req) => response.map_err(ExecutorStreamError::OpenAI)?,
            };
            let mut heartbeat_sleep =
                (heartbeat.as_secs() > 0).then(|| Box::pin(tokio::time::sleep(heartbeat)));

            loop {
                let item = if let Some(ref mut heartbeat_sleep) = heartbeat_sleep {
                    tokio::select! {
                        () = ctx.cancelled() => return Err(ExecutorStreamError::Cancelled),
                        () = heartbeat_sleep.as_mut() => {
                            tracing::debug!(
                                elapsed_ms = attempt_started.elapsed().as_millis(),
                                chars_so_far = stream_state.content.len(),
                                chunks = stream_state.chunks,
                                "executor stream heartbeat"
                            );
                            heartbeat_sleep
                                .as_mut()
                                .reset(tokio::time::Instant::now() + heartbeat);
                            continue;
                        }
                        item = stream.next() => item,
                    }
                } else {
                    tokio::select! {
                        () = ctx.cancelled() => return Err(ExecutorStreamError::Cancelled),
                        item = stream.next() => item,
                    }
                };

                match item {
                    None => break,
                    Some(Ok(chunk)) => {
                        stream_state.chunks += 1;

                        if stream_state.response_id.is_none() {
                            stream_state.response_id = Some(chunk.id.clone());
                        }

                        if let Some(usage) = chunk.usage {
                            stream_state.usage = Some(usage);
                        }

                        for choice in chunk.choices {
                            if let Some(finish_reason) = choice.finish_reason {
                                stream_state.finish_reason = Some(finish_reason);
                            }
                            if let Some(delta) = choice.delta.content
                                && !delta.is_empty()
                            {
                                if stream_state.first_content_ms.is_none() {
                                    stream_state.first_content_ms =
                                        Some(attempt_started.elapsed().as_millis());
                                }
                                stream_state.content.push_str(&delta);
                            }
                        }

                        if let Some(ref mut heartbeat_sleep) = heartbeat_sleep {
                            heartbeat_sleep
                                .as_mut()
                                .reset(tokio::time::Instant::now() + heartbeat);
                        }
                    }
                    Some(Err(e)) => return Err(ExecutorStreamError::OpenAI(e)),
                }
            }

            Ok(())
        })
        .await;

        let duration = attempt_started.elapsed();
        let usage = stream_state
            .usage
            .as_ref()
            .map(crate::logging::token_usage_from_completion_usage);

        match stream_result {
            Ok(Ok(())) => {
                tracing::debug!(
                    duration_ms = duration.as_millis(),
                    chunks = stream_state.chunks,
                    chars = stream_state.content.len(),
                    "Executor stream completed"
                );

                if !stream_state.has_content() {
                    let summary = Some(executor_stream_summary(
                        attempt,
                        duration,
                        &stream_state,
                        ExecutorStreamOutcome {
                            partial: false,
                            timeout: false,
                            empty: true,
                            stream_error: None,
                            stream_error_class: None,
                        },
                    ));
                    let attempt_secs = duration.as_secs();

                    if attempt == 0 && attempt_secs <= cfg.empty_response_no_retry_after_secs {
                        tracing::warn!(
                            duration_secs = attempt_secs,
                            threshold_secs = cfg.empty_response_no_retry_after_secs,
                            "Executor stream completed empty; retrying once"
                        );
                        continue;
                    }

                    let err_msg = if attempt == 0
                        && attempt_secs > cfg.empty_response_no_retry_after_secs
                    {
                        format!(
                            "Reasoning model returned no response after a long attempt ({attempt_secs}s); retry suppressed."
                        )
                    } else {
                        format!(
                            "Reasoning model returned no response after {} attempt(s). Possible causes: content filtering, prompt issues, or API anomaly.",
                            attempt + 1
                        )
                    };

                    tracing::error!("{}", err_msg);
                    log_record(
                        false,
                        Some(format!("stage=empty_response: {err_msg}")),
                        None,
                        Some(executor_model.to_string()),
                        usage,
                        files.len(),
                        summary,
                    );
                    return Err(ToolError::internal(err_msg));
                }

                let summary = Some(executor_stream_summary(
                    attempt,
                    duration,
                    &stream_state,
                    ExecutorStreamOutcome {
                        partial: false,
                        timeout: false,
                        empty: false,
                        stream_error: None,
                        stream_error_class: None,
                    },
                ));
                return finalize_executor_success(stream_state.content, usage, false, summary);
            }
            Ok(Err(ExecutorStreamError::Cancelled)) => return Err(ToolError::cancelled(None)),
            Ok(Err(ExecutorStreamError::OpenAI(e))) => {
                let error_text = e.to_string();
                let stream_error_class = stream_error_class_from_openai(&e);
                if stream_state.has_content() {
                    tracing::warn!(
                        error = %e,
                        chars = stream_state.content.len(),
                        chunks = stream_state.chunks,
                        "Executor stream failed after partial content; salvaging"
                    );
                    let summary = Some(executor_stream_summary(
                        attempt,
                        duration,
                        &stream_state,
                        ExecutorStreamOutcome {
                            partial: true,
                            timeout: false,
                            empty: false,
                            stream_error: Some(&error_text),
                            stream_error_class: Some(stream_error_class),
                        },
                    ));
                    return finalize_executor_success(stream_state.content, usage, true, summary);
                }

                let retryable = crate::errors::is_retryable_app_level(&e);
                if attempt < EXECUTOR_RETRIES && retryable {
                    tracing::warn!("Executor stream failed with retryable error: {e}; retrying...");
                    continue;
                }

                if retryable {
                    tracing::error!(
                        "Executor stream failed after {} attempts with retryable error: {}",
                        attempt + 1,
                        e
                    );
                } else {
                    tracing::error!("Executor stream failed with non-retryable error: {}", e);
                }

                let summary = Some(executor_stream_summary(
                    attempt,
                    duration,
                    &stream_state,
                    ExecutorStreamOutcome {
                        partial: false,
                        timeout: false,
                        empty: false,
                        stream_error: Some(&error_text),
                        stream_error_class: Some(stream_error_class),
                    },
                ));
                let msg = format!("stage=chat_execute: {e}");
                log_record(
                    false,
                    Some(msg),
                    None,
                    Some(executor_model.to_string()),
                    usage,
                    files.len(),
                    summary,
                );
                return Err(ToolError::from(ReasonerError::from(e)));
            }
            Err(_) => {
                if stream_state.has_content() {
                    tracing::warn!(
                        timeout_secs = cfg.executor_timeout_secs,
                        chars = stream_state.content.len(),
                        chunks = stream_state.chunks,
                        "Executor stream timed out after partial content; salvaging"
                    );
                    let summary = Some(executor_stream_summary(
                        attempt,
                        duration,
                        &stream_state,
                        ExecutorStreamOutcome {
                            partial: true,
                            timeout: true,
                            empty: false,
                            stream_error: Some("executor_timeout"),
                            stream_error_class: Some("timeout"),
                        },
                    ));
                    return finalize_executor_success(stream_state.content, usage, true, summary);
                }

                let err_msg = format!(
                    "Executor stream timed out after {} second(s) before any content arrived.",
                    cfg.executor_timeout_secs
                );
                tracing::error!("{}", err_msg);
                let summary = Some(executor_stream_summary(
                    attempt,
                    duration,
                    &stream_state,
                    ExecutorStreamOutcome {
                        partial: false,
                        timeout: true,
                        empty: false,
                        stream_error: Some("executor_timeout"),
                        stream_error_class: Some("timeout"),
                    },
                ));
                log_record(
                    false,
                    Some(format!("stage=executor_timeout: {err_msg}")),
                    None,
                    Some(executor_model.to_string()),
                    usage,
                    files.len(),
                    summary,
                );
                return Err(ToolError::external(err_msg));
            }
        }
    }

    // Should never reach here due to loop logic, but provide a defensive error
    log_record(
        false,
        Some("stage=final_unreachable: Executor failed after all retries".to_string()),
        None,
        Some(executor_model.to_string()),
        None,
        files.len(),
        None,
    );
    Err(ToolError::Internal(
        "Executor failed after all retries".to_string(),
    ))
}

#[cfg(test)]
#[expect(
    clippy::allow_attributes,
    reason = "incremental legacy lint mitigation for pre-existing tests"
)]
// TODO(3): clean up unwrap_used as part of broader gpt5_reasoner lint conformance pass.
#[allow(clippy::unwrap_used)]
mod retry_tests {
    use super::*;
    use crate::engine::preflight;
    use crate::errors::ReasonerError;
    use crate::test_support::EnvGuard;
    use agentic_config::types::ReasoningConfig;
    use serial_test::serial;
    use std::fs;
    use std::fs::OpenOptions;
    use tempfile::TempDir;

    #[test]
    fn test_template_error_is_retryable() {
        let template_err = ReasonerError::Template("missing marker".into());
        assert!(matches!(template_err, ReasonerError::Template(_)));
    }

    #[test]
    fn test_yaml_error_is_not_template_error() {
        // Create a YAML error by parsing invalid YAML
        let yaml_result: std::result::Result<serde_yaml::Value, _> =
            serde_yaml::from_str("invalid: yaml: syntax");
        assert!(yaml_result.is_err());

        let yaml_err = ReasonerError::Yaml(yaml_result.unwrap_err());
        assert!(!matches!(yaml_err, ReasonerError::Template(_)));
    }

    #[test]
    fn executor_stream_summary_emits_descriptive_diagnostics_only() {
        let usage: CompletionUsage = serde_json::from_value(serde_json::json!({
            "prompt_tokens": 11,
            "completion_tokens": 21,
            "total_tokens": 32,
            "completion_tokens_details": { "reasoning_tokens": 8 }
        }))
        .unwrap();

        let state = ExecutorStreamState {
            content: "abc".into(),
            usage: Some(usage),
            chunks: 3,
            first_content_ms: Some(15),
            response_id: Some("resp_123".into()),
            finish_reason: Some(FinishReason::Stop),
        };

        let summary = executor_stream_summary(
            0,
            std::time::Duration::from_millis(100),
            &state,
            ExecutorStreamOutcome {
                partial: false,
                timeout: false,
                empty: false,
                stream_error: None,
                stream_error_class: None,
            },
        );

        assert_eq!(summary["attempt_index"], 0);
        assert_eq!(summary["attempt_duration_ms"], 100);
        assert_eq!(summary["chunks_received"], 3);
        assert_eq!(summary["time_to_first_content_ms"], 15);
        assert_eq!(summary["usage_chunk_observed"], true);
        assert_eq!(summary["response_id"], "resp_123");
        assert_eq!(summary["finish_reason"], "stop");
        assert_eq!(summary["completion_tokens_details"]["reasoning_tokens"], 8);
        assert!(summary.get("stream_error_class").is_some());
        assert!(summary.get("attempt").is_none());
        assert!(summary.get("duration_ms").is_none());
        assert!(summary.get("chunks").is_none());
        assert!(summary.get("first_content_ms").is_none());
        assert!(summary.get("usage_present").is_none());
    }

    #[tokio::test]
    async fn pre_cancelled_context_returns_cancelled_before_api_setup() {
        let ctx = agentic_tools_core::ToolContext::default();
        ctx.cancellation_token().cancel();

        let result = gpt5_reasoner_impl(
            "test".to_string(),
            vec![FileMeta {
                filename: "/definitely/not/present.txt".into(),
                description: "missing file that should never be prechecked".into(),
            }],
            Some(vec![DirectoryMeta {
                directory_path: "/definitely/not/a/real/directory".into(),
                description: "directory expansion should be skipped".into(),
                extensions: None,
                recursive: true,
                include_hidden: false,
                max_files: 1000,
            }]),
            &ReasoningConfig::default(),
            PromptType::Reasoning,
            None,
            &ctx,
        )
        .await;

        assert!(matches!(result, Err(ToolError::Cancelled { .. })));
    }

    #[tokio::test]
    #[serial(env)]
    async fn aggregate_preflight_file_limit_fails_before_missing_env() {
        let _api_key = EnvGuard::remove("OPENROUTER_API_KEY");
        let _inject = EnvGuard::set("INJECT_CLAUDE_MD", "0");

        let files = (0..=preflight::MAX_UNIQUE_FILES)
            .map(|idx| FileMeta {
                filename: format!("/tmp/nonexistent-{idx}.rs"),
                description: "desc".into(),
            })
            .collect::<Vec<_>>();

        let err = gpt5_reasoner_impl(
            "test".to_string(),
            files,
            None,
            &ReasoningConfig::default(),
            PromptType::Reasoning,
            None,
            &ToolContext::default(),
        )
        .await
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("stage=preflight_aggregate"));
        assert!(msg.contains("500"));
        assert!(!msg.contains("Missing environment variable"));
    }

    #[tokio::test]
    #[serial(env)]
    async fn aggregate_preflight_byte_limit_fails_before_missing_env() {
        let _api_key = EnvGuard::remove("OPENROUTER_API_KEY");
        let _inject = EnvGuard::set("INJECT_CLAUDE_MD", "0");
        let td = TempDir::new().unwrap();
        let file = td.path().join("large.txt");
        fs::write(&file, "x").unwrap();
        OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_len(preflight::MAX_FS_BYTES + 1)
            .unwrap();

        let err = gpt5_reasoner_impl(
            "test".to_string(),
            vec![FileMeta {
                filename: file.to_string_lossy().to_string(),
                description: "large file".into(),
            }],
            None,
            &ReasoningConfig::default(),
            PromptType::Reasoning,
            None,
            &ToolContext::default(),
        )
        .await
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("stage=preflight_aggregate"));
        assert!(msg.contains("25 MiB"));
        assert!(!msg.contains("Missing environment variable"));
    }

    #[tokio::test]
    #[serial(env)]
    async fn aggregate_preflight_token_estimate_limit_fails_before_missing_env() {
        let _api_key = EnvGuard::remove("OPENROUTER_API_KEY");
        let _inject = EnvGuard::set("INJECT_CLAUDE_MD", "0");
        let td = TempDir::new().unwrap();
        let file = td.path().join("small.txt");
        fs::write(&file, "tiny").unwrap();

        let err = gpt5_reasoner_impl(
            "test".to_string(),
            vec![FileMeta {
                filename: file.to_string_lossy().to_string(),
                description: "word ".repeat(preflight::MAX_OPTIMIZER_PROMPT_TOKENS_EST),
            }],
            None,
            &ReasoningConfig::default(),
            PromptType::Reasoning,
            None,
            &ToolContext::default(),
        )
        .await
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("stage=preflight_aggregate"));
        assert!(msg.contains("60000"));
        assert!(!msg.contains("Missing environment variable"));
    }
}
