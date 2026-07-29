#![expect(
    clippy::unwrap_used,
    reason = "security audit fixtures should fail immediately"
)]

use claudecode::MCPServer;
use coding_agent_tools::agent;
use coding_agent_tools::types::AgentLocation;
use coding_agent_tools::types::AgentType;

fn cells() -> [(AgentType, AgentLocation); 8] {
    [
        (AgentType::Locator, AgentLocation::Codebase),
        (AgentType::Locator, AgentLocation::Thoughts),
        (AgentType::Locator, AgentLocation::References),
        (AgentType::Locator, AgentLocation::Web),
        (AgentType::Analyzer, AgentLocation::Codebase),
        (AgentType::Analyzer, AgentLocation::Thoughts),
        (AgentType::Analyzer, AgentLocation::References),
        (AgentType::Analyzer, AgentLocation::Web),
    ]
}

#[test]
fn exact_matrix_has_zero_builtins_and_no_write_capability() {
    let approved = std::collections::HashSet::from([
        "cli_ls",
        "cli_grep",
        "cli_glob",
        "workspace_read",
        "workspace_todowrite",
        "thoughts_list_documents",
        "thoughts_read_document",
        "thoughts_list_references",
        "thoughts_read_reference",
        "web_search",
        "web_fetch",
    ]);
    for (agent_type, location) in cells() {
        let tools = agent::enabled_tools_for(agent_type, location);
        assert!(!tools.is_empty());
        assert!(
            tools
                .iter()
                .all(|tool| tool.starts_with("mcp__agentic-mcp__"))
        );
        assert!(tools.iter().all(|tool| {
            tool.strip_prefix("mcp__agentic-mcp__")
                .is_some_and(|name| approved.contains(name))
        }));

        let config = agent::build_mcp_config(location, &tools);
        assert_eq!(config.mcp_servers.len(), 1);
        let MCPServer::Stdio { command, args, env } = &config.mcp_servers["agentic-mcp"] else {
            panic!("production server must use stdio");
        };
        assert_eq!(command, "agentic-mcp");
        assert!(args.windows(2).any(|pair| pair[0] == "--nested-profile"));
        assert!(args.windows(2).any(|pair| pair[0] == "--allow"));
        assert!(
            env.as_ref()
                .is_none_or(|env| !env.contains_key("ENABLE_TOOL_SEARCH"))
        );
    }
}

#[test]
fn web_cells_have_no_local_or_editing_tools() {
    for agent_type in [AgentType::Locator, AgentType::Analyzer] {
        let tools = agent::enabled_tools_for(agent_type, AgentLocation::Web);
        assert!(tools.iter().all(|tool| {
            !tool.contains("cli_")
                && !tool.contains("workspace_read")
                && !tool.contains("thoughts_")
                && !tool.contains("edit")
                && !tool.contains("apply_patch")
        }));
    }
}

#[test]
fn parent_opencode_structure_has_no_workspace_expansion() {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../../../opencode.json")).unwrap();
    let tools = config["tools"].as_object().unwrap();
    assert!(
        !tools
            .keys()
            .any(|name| name.starts_with("tools_workspace_"))
    );
    assert_eq!(
        tools.get("tools_cli_*"),
        Some(&serde_json::Value::Bool(true))
    );
    assert_eq!(
        tools.get("tools_thoughts_*"),
        Some(&serde_json::Value::Bool(true))
    );
    assert_eq!(
        config.pointer("/mcp/tools/command").unwrap(),
        &serde_json::json!(["agentic-mcp"])
    );
}

#[test]
fn acceptance_requirements_have_exact_stable_coverage_ids() {
    struct Coverage<'a> {
        id: &'a str,
        tests: &'a [&'a str],
        source: &'a str,
    }

    const SDK_CONFIG: &str = include_str!("../../../services/claudecode-rs/src/config.rs");
    const SDK_FIXTURES: &str = concat!(
        include_str!("../../../services/claudecode-rs/src/types.rs"),
        include_str!("../../../services/claudecode-rs/src/session.rs"),
        include_str!("../../../services/claudecode-rs/tests/integration.rs")
    );
    const WORKSPACE: &str = include_str!("../../workspace-tools/src/tools.rs");
    const BINARY: &str =
        include_str!("../../../../apps/agentic-mcp/tests/nested_profile_publication.rs");
    const STDOUT: &str = include_str!("../../../../apps/agentic-mcp/tests/stdout_cleanliness.rs");
    const CONFIG: &str = include_str!("../src/agent/config.rs");
    const GUIDANCE: &str = include_str!("../src/agent/guidance.rs");
    const EVIDENCE: &str = include_str!("../src/agent/evidence.rs");
    const LIVE: &str = include_str!("../src/agent/live_tests.rs");
    const SANDBOX: &str = include_str!("sandbox_roots.rs");
    const CONFIG_AUDIT: &str = concat!(
        include_str!("../src/agent/config.rs"),
        include_str!("security_audit.rs")
    );
    const BINARY_THOUGHTS: &str = concat!(
        include_str!("../../../../apps/agentic-mcp/tests/nested_profile_publication.rs"),
        include_str!("../../thoughts-mcp-tools/src/tools.rs")
    );
    const AUDIT_BINARY: &str = concat!(
        include_str!("security_audit.rs"),
        include_str!("../../../../apps/agentic-mcp/tests/nested_profile_publication.rs")
    );
    const SDK_ORCHESTRATION: &str = concat!(
        include_str!("../../../services/claudecode-rs/tests/integration.rs"),
        include_str!("../src/lib.rs")
    );

    const REQUIRED_IDS: &[&str] = &[
        "STATIC-01",
        "STATIC-02",
        "STATIC-03",
        "STATIC-04",
        "STATIC-05",
        "STATIC-06",
        "STATIC-07",
        "PROCESS-01",
        "PROCESS-02",
        "PROCESS-03",
        "PROCESS-04",
        "PROCESS-05",
        "PROCESS-06",
        "CONTROL-01",
        "CONTROL-02",
        "CONTROL-03",
        "CONTROL-04",
        "CONTROL-05",
        "LIVE-01",
        "LIVE-02",
        "LIVE-03",
        "LIVE-04",
        "LIVE-05",
        "LIVE-06",
        "LIVE-07",
        "HERMETIC-01",
        "HERMETIC-02",
        "HERMETIC-03",
    ];
    let coverage = [
        Coverage {
            id: "STATIC-01",
            tests: &[
                "exact_eight_cell_matrix_is_locked",
                "exact_matrix_has_zero_builtins_and_no_write_capability",
                "web_cells_have_no_local_or_editing_tools",
            ],
            source: CONFIG_AUDIT,
        },
        Coverage {
            id: "STATIC-02",
            tests: &["mcp_always_load_policy_applies_during_full_config_serialization"],
            source: SDK_CONFIG,
        },
        Coverage {
            id: "STATIC-03",
            tests: &["test_model_for_rejects_unknown_values"],
            source: CONFIG,
        },
        Coverage {
            id: "STATIC-04",
            tests: &[
                "includes_only_tracked_guidance_in_deterministic_scope_order",
                "rejects_tracked_regular_file_replaced_by_final_symlink",
                "rejects_guidance_through_intermediate_symlink_inside_worktree",
            ],
            source: GUIDANCE,
        },
        Coverage {
            id: "STATIC-05",
            tests: &[
                "workspace_read_rejects_oversized_files_before_reading",
                "workspace_todowrite_validates_count_content_and_duplicates",
            ],
            source: WORKSPACE,
        },
        Coverage {
            id: "STATIC-06",
            tests: &[
                "test_event_deserialization",
                "fake_claude_retains_raw_transcript_and_warning_diagnostics",
                "fake_claude_redacts_configured_secrets_from_tool_results_and_stderr",
            ],
            source: SDK_FIXTURES,
        },
        Coverage {
            id: "STATIC-07",
            tests: &["stream_json_input_is_rejected"],
            source: SDK_CONFIG,
        },
        Coverage {
            id: "PROCESS-01",
            tests: &[
                "codebase_profile_publishes_only_allowlisted_read_capabilities",
                "dangerous_nested_allowlist_names_publish_exactly_nothing",
            ],
            source: BINARY,
        },
        Coverage {
            id: "PROCESS-02",
            tests: &[
                "default_parent_does_not_publish_runtime_only_tools",
                "parent_opencode_structure_has_no_workspace_expansion",
            ],
            source: AUDIT_BINARY,
        },
        Coverage {
            id: "PROCESS-03",
            tests: &["serving_diagnostics_never_use_protocol_stdout"],
            source: STDOUT,
        },
        Coverage {
            id: "PROCESS-04",
            tests: &["deterministic_mcp_preflight_honors_working_directory"],
            source: SDK_FIXTURES,
        },
        Coverage {
            id: "PROCESS-05",
            tests: &[
                "thoughts_profile_resolves_mounted_active_work_without_mutation",
                "references_profile_resolves_configured_mount_and_exact_publication",
                "specialized_read_rejects_symlink_escape",
            ],
            source: BINARY_THOUGHTS,
        },
        Coverage {
            id: "PROCESS-06",
            tests: &[
                "sandbox_accepts_secondary_root_and_rejects_outside_paths",
                "sandbox_relative_paths_always_resolve_under_first_worktree_root",
                "sandbox_never_returns_symlink_escape_name",
            ],
            source: SANDBOX,
        },
        Coverage {
            id: "CONTROL-01",
            tests: &["live_control_deferred_empty_builtins_cannot_call_mcp"],
            source: LIVE,
        },
        Coverage {
            id: "CONTROL-02",
            tests: &["live_control_deferred_isolated_toolsearch_discovers_mcp"],
            source: LIVE,
        },
        Coverage {
            id: "CONTROL-03",
            tests: &["live_control_inherited_toolsearch_deny_rejects_fabricated_prose"],
            source: LIVE,
        },
        Coverage {
            id: "CONTROL-04",
            tests: &["live_control_eager_empty_builtins_calls_directly_without_toolsearch"],
            source: LIVE,
        },
        Coverage {
            id: "CONTROL-05",
            tests: &["production_mcp_config_never_sets_global_eager_environment"],
            source: LIVE,
        },
        Coverage {
            id: "LIVE-01",
            tests: &[
                "live_locator_codebase_calls_all_discovery_tools_with_hermetic_guidance",
                "live_analyzer_codebase_calls_read_discovery_and_todo",
                "live_locator_thoughts_calls_listing_and_all_discovery_tools",
                "live_analyzer_thoughts_calls_specialized_read_and_discovery",
                "live_locator_references_calls_listing_and_all_discovery_tools",
                "live_analyzer_references_calls_specialized_read_discovery_and_todo",
                "live_locator_web_calls_search_and_fetch_without_local_access",
                "live_analyzer_web_calls_search_fetch_and_todo_without_local_access",
            ],
            source: LIVE,
        },
        Coverage {
            id: "LIVE-02",
            tests: &[
                "live_locator_codebase_calls_all_discovery_tools_with_hermetic_guidance",
                "live_analyzer_codebase_calls_read_discovery_and_todo",
            ],
            source: LIVE,
        },
        Coverage {
            id: "LIVE-03",
            tests: &[
                "live_locator_thoughts_calls_listing_and_all_discovery_tools",
                "live_analyzer_thoughts_calls_specialized_read_and_discovery",
            ],
            source: LIVE,
        },
        Coverage {
            id: "LIVE-04",
            tests: &[
                "live_locator_references_calls_listing_and_all_discovery_tools",
                "live_analyzer_references_calls_specialized_read_discovery_and_todo",
            ],
            source: LIVE,
        },
        Coverage {
            id: "LIVE-05",
            tests: &[
                "live_locator_web_calls_search_and_fetch_without_local_access",
                "live_analyzer_web_calls_search_fetch_and_todo_without_local_access",
            ],
            source: LIVE,
        },
        Coverage {
            id: "LIVE-06",
            tests: &["ci_fixture_proves_nonce_backed_evidence_for_every_cell"],
            source: LIVE,
        },
        Coverage {
            id: "LIVE-07",
            tests: &[
                "rejects_unpaired_errored_todo_and_disallowed_evidence",
                "rejects_wrong_role_tool_blocks",
                "rejects_leftover_unpaired_tool_use_after_valid_evidence",
                "nonce_requires_successful_paired_expected_tool_result_and_prompt_absence",
            ],
            source: EVIDENCE,
        },
        Coverage {
            id: "HERMETIC-01",
            tests: &["live_locator_codebase_calls_all_discovery_tools_with_hermetic_guidance"],
            source: LIVE,
        },
        Coverage {
            id: "HERMETIC-02",
            tests: &["fake_claude_empty_setting_sources_exclude_user_home_ambient_state"],
            source: SDK_FIXTURES,
        },
        Coverage {
            id: "HERMETIC-03",
            tests: &[
                "fake_claude_redacts_configured_secrets_from_tool_results_and_stderr",
                "ask_agent_config_is_hermetic_and_zero_builtin",
            ],
            source: SDK_ORCHESTRATION,
        },
    ];

    let required = REQUIRED_IDS
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let mut mapped = std::collections::HashSet::new();
    for entry in coverage {
        assert!(
            required.contains(entry.id),
            "unknown requirement ID: {}",
            entry.id
        );
        assert!(
            mapped.insert(entry.id),
            "duplicate requirement ID: {}",
            entry.id
        );
        assert!(
            !entry.tests.is_empty(),
            "requirement {} has no tests",
            entry.id
        );
        for test in entry.tests {
            assert!(
                entry.source.contains(&format!("fn {test}")),
                "mapped test does not exist: {} -> {test}",
                entry.id
            );
        }
    }
    assert_eq!(
        mapped, required,
        "missing stable acceptance requirement IDs"
    );
}
