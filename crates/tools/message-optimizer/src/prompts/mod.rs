use crate::types::OptimizeMessageRequest;

pub const OPTIMIZER_SYSTEM: &str = include_str!("optimizer_system.md");

pub fn build_optimizer_user(request: &OptimizeMessageRequest) -> String {
    let supplemental_context = request
        .supplemental_context
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(|| "(none)".to_string(), str::to_owned);

    format!(
        concat!(
            "Optimize the following agent-style message for GPT-5.4 xhigh.\n\n",
            "Return only the required tool call with:\n",
            "- system_prompt: reusable standing instructions\n",
            "- user_prompt: task-specific prompt content\n\n",
            "<message>\n{message}\n</message>\n\n",
            "<supplemental_context>\n{supplemental_context}\n</supplemental_context>\n"
        ),
        message = request.message.trim(),
        supplemental_context = supplemental_context,
    )
}

pub fn build_repair_user(request: &OptimizeMessageRequest, contract_error: &str) -> String {
    format!(
        concat!(
            "Your previous response violated the output contract.\n",
            "Repair it by calling the required tool exactly once with valid non-empty strings for ",
            "system_prompt and user_prompt.\n\n",
            "<contract_error>\n{contract_error}\n</contract_error>\n\n",
            "<original_request>\n{original_request}\n</original_request>\n"
        ),
        contract_error = contract_error.trim(),
        original_request = build_optimizer_user(request),
    )
}
