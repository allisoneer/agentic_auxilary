mod bridge;
mod dto;
mod error;
mod mutation;
pub(crate) mod supervisor;

use bridge::desktop_acknowledge_attention_signal;
use bridge::desktop_acknowledge_change;
use bridge::desktop_acknowledge_reminder_fire;
use bridge::desktop_acknowledge_snapshot;
use bridge::desktop_cancel_work_item;
use bridge::desktop_complete_work_item;
use bridge::desktop_create_reminder;
use bridge::desktop_create_work_item;
use bridge::desktop_snooze_reminder_fire;
use bridge::desktop_state;
use supervisor::DesktopSupervisor;
use tauri::Manager;
use tauri::RunEvent;

fn initialize_supervisor<R: tauri::Runtime>(
    handle: &tauri::AppHandle<R>,
    url: String,
) -> Result<(), String> {
    let app_handle = handle.clone();
    let supervisor =
        tauri::async_runtime::block_on(async move { DesktopSupervisor::start(app_handle, url) })
            .map_err(|error| format!("{}: {}", error.category, error.message))?;
    handle.manage(supervisor);
    Ok(())
}

fn shutdown_supervisor<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) {
    if let Some(supervisor) = handle.try_state::<DesktopSupervisor>() {
        let _ = tauri::async_runtime::block_on(supervisor.close());
    }
}

pub fn builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(|app| {
            let url = std::env::var("ATTENTION_SERVER_URL")
                .unwrap_or_else(|_| "ws://127.0.0.1:8787/v1/ws".to_string());
            initialize_supervisor(app.handle(), url)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            desktop_state,
            desktop_acknowledge_snapshot,
            desktop_acknowledge_change,
            desktop_create_work_item,
            desktop_complete_work_item,
            desktop_cancel_work_item,
            desktop_acknowledge_attention_signal,
            desktop_create_reminder,
            desktop_acknowledge_reminder_fire,
            desktop_snooze_reminder_fire
        ])
}

pub fn run() {
    #[expect(
        clippy::exit,
        reason = "Tauri context macro handles invalid generated configuration"
    )]
    let context = tauri::generate_context!();
    let app = builder()
        .build(context)
        .unwrap_or_else(|error| panic!("failed to build Attention desktop: {error}"));
    app.run(|handle, event| {
        if matches!(event, RunEvent::ExitRequested { .. }) {
            shutdown_supervisor(handle);
        }
    });
}

#[cfg(test)]
mod live_tests;

#[cfg(test)]
mod tests {
    use crate::dto::AffectedViewDto;
    use crate::dto::ChangeEventDto;
    use crate::dto::ConnectionStatusDto;
    use crate::dto::DesktopMessageDto;
    use crate::dto::InboxEffectsDto;
    use crate::dto::InboxEntryDto;
    use crate::dto::ResetReason;
    use crate::dto::SnapshotDto;
    use crate::dto::WorkItemDto;
    use crate::error::DesktopErrorDto;
    use crate::supervisor::DesktopSupervisor;
    use crate::supervisor::TestMutationCall;
    use crate::supervisor::TestMutationResult;
    use attention_client::ClientError;
    use attention_client::ConnectionStatus;
    use attention_protocol as protocol;
    use attention_protocol::Revision;
    use attention_protocol::WorkItemId;
    use attention_protocol::WorkItemLifecycle;
    use attention_protocol::WorkItemView;
    use serde_json::Value;
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use tauri::Manager;
    use tauri::ipc::CallbackFn;
    use tauri::ipc::InvokeBody;
    use tauri::test::INVOKE_KEY;
    use tauri::test::get_ipc_response;
    use tauri::test::mock_builder;
    use tauri::test::mock_context;
    use tauri::webview::InvokeRequest;

    fn request(command: &str, body: Value) -> InvokeRequest {
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("invoke URL"),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        }
    }

    fn app_with(supervisor: DesktopSupervisor) -> tauri::App<tauri::test::MockRuntime> {
        mock_builder()
            .manage(supervisor)
            .invoke_handler(tauri::generate_handler![
                crate::bridge::desktop_state,
                crate::bridge::desktop_acknowledge_snapshot,
                crate::bridge::desktop_acknowledge_change,
                crate::bridge::desktop_create_work_item,
                crate::bridge::desktop_complete_work_item,
                crate::bridge::desktop_cancel_work_item,
                crate::bridge::desktop_acknowledge_attention_signal,
                crate::bridge::desktop_create_reminder,
                crate::bridge::desktop_acknowledge_reminder_fire,
                crate::bridge::desktop_snooze_reminder_fire
            ])
            .build(mock_context(tauri::test::noop_assets()))
            .expect("mock Tauri app")
    }

    fn invoke(
        webview: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        command: &str,
        body: Value,
    ) -> Result<Value, Value> {
        get_ipc_response(webview, request(command, body))
            .map(|response| response.deserialize().expect("JSON response"))
    }

    fn work_result(id: &str) -> protocol::CreateWorkItemResult {
        protocol::MutationResult {
            disposition: protocol::MutationDisposition::Applied,
            value: protocol::WorkItemMutationValue {
                id: protocol::WorkItemId(id.into()),
            },
            cursor: protocol::Cursor(format!("cursor-{id}")),
            change_event_id: protocol::ChangeEventId(format!("event-{id}")),
            outbox_intent_id: Some(protocol::OutboxIntentId("private-outbox".into())),
        }
    }

    fn reminder_result(reminder_id: &str, fire_id: &str) -> protocol::CreateReminderResult {
        protocol::MutationResult {
            disposition: protocol::MutationDisposition::Applied,
            value: protocol::ReminderMutationValue {
                reminder_id: protocol::ReminderId(reminder_id.into()),
                fire_id: protocol::ReminderFireId(fire_id.into()),
            },
            cursor: protocol::Cursor(format!("cursor-{fire_id}")),
            change_event_id: protocol::ChangeEventId(format!("event-{fire_id}")),
            outbox_intent_id: Some(protocol::OutboxIntentId("private-outbox".into())),
        }
    }

    fn is_v7(value: &str) -> bool {
        uuid::Uuid::parse_str(value).is_ok_and(|id| id.get_version_num() == 7)
    }

    fn ordered_snapshot() -> SnapshotDto {
        let item = |id: &str, revision: &str| WorkItemDto {
            id: id.into(),
            revision: revision.into(),
            lifecycle: WorkItemLifecycle::Open,
            due_at: None,
            scheduled_at: None,
            defer_until: None,
        };
        SnapshotDto {
            work_items: vec![item("first", "1"), item("second", "1")],
            attention_signals: vec![],
            reminders: vec![],
            inbox: vec![
                InboxEntryDto::WorkItem {
                    work_item: item("first", "1"),
                },
                InboxEntryDto::WorkItem {
                    work_item: item("second", "1"),
                },
            ],
        }
    }

    fn updating_change(cursor: &str) -> ChangeEventDto {
        let updated = WorkItemDto {
            id: "first".into(),
            revision: "2".into(),
            lifecycle: WorkItemLifecycle::Completed,
            due_at: None,
            scheduled_at: None,
            defer_until: None,
        };
        ChangeEventDto {
            id: "change".into(),
            cursor: cursor.into(),
            occurred_at: protocol::WireTimestamp::parse("2026-08-14T00:00:00.000000Z")
                .expect("timestamp"),
            kind: protocol::ChangeKind::WorkItemCompleted,
            affected: vec![AffectedViewDto::WorkItem {
                work_item: updated.clone(),
            }],
            inbox: InboxEffectsDto {
                upserts: vec![InboxEntryDto::WorkItem { work_item: updated }],
                removals: vec![],
            },
        }
    }

    #[test]
    fn mock_ipc_invokes_all_mutations_with_exact_typed_params_and_sanitized_receipts() {
        let (supervisor, transport) = DesktopSupervisor::for_test(1, None, &[], VecDeque::new());
        transport.script([
            TestMutationResult::WorkItem(Ok(work_result("created"))),
            TestMutationResult::WorkItem(Ok(work_result("work"))),
            TestMutationResult::WorkItem(Ok(work_result("work"))),
            TestMutationResult::Signal(Ok(protocol::MutationResult {
                disposition: protocol::MutationDisposition::Replayed,
                value: protocol::AttentionSignalMutationValue {
                    id: protocol::AttentionSignalId("signal".into()),
                },
                cursor: protocol::Cursor("cursor-signal".into()),
                change_event_id: protocol::ChangeEventId("event-signal".into()),
                outbox_intent_id: Some(protocol::OutboxIntentId("private-outbox".into())),
            })),
            TestMutationResult::Reminder(Ok(reminder_result("reminder", "initial"))),
            TestMutationResult::Reminder(Ok(reminder_result("reminder", "fire"))),
            TestMutationResult::Reminder(Ok(reminder_result("reminder", "replacement"))),
        ]);
        let app = app_with(supervisor);
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock webview");
        let calls = [
            (
                "desktop_create_work_item",
                serde_json::json!({"input":{"dueAt":"2026-08-14T01:02:03.123456+02:00","scheduledAt":null,"deferUntil":"2026-08-15T00:00:00Z"}}),
            ),
            (
                "desktop_complete_work_item",
                serde_json::json!({"input":{"id":"work","expectedRevision":"7"}}),
            ),
            (
                "desktop_cancel_work_item",
                serde_json::json!({"input":{"id":"work","expectedRevision":"8"}}),
            ),
            (
                "desktop_acknowledge_attention_signal",
                serde_json::json!({"input":{"id":"signal","expectedRevision":"9"}}),
            ),
            (
                "desktop_create_reminder",
                serde_json::json!({"input":{"target":{"kind":"work_item","workItemId":"work"},"triggerAt":"2026-08-14T01:02:03.123456+02:00"}}),
            ),
            (
                "desktop_acknowledge_reminder_fire",
                serde_json::json!({"input":{"reminderId":"reminder","fireId":"fire","expectedRevision":"10"}}),
            ),
            (
                "desktop_snooze_reminder_fire",
                serde_json::json!({"input":{"reminderId":"reminder","fireId":"fire","expectedRevision":"11","replacementTriggerAt":"2026-08-16T01:02:03.123456Z"}}),
            ),
        ];
        for (command, body) in calls {
            let receipt = invoke(&webview, command, body).expect(command);
            assert!(receipt.get("cursor").is_some() && receipt.get("changeEventId").is_some());
            assert!(receipt.get("outboxIntentId").is_none());
            assert!(!receipt.to_string().contains("private-outbox"));
        }
        let calls = transport.mutation_calls.lock().expect("calls");
        assert_eq!(calls.len(), 7);
        match &calls[0] {
            TestMutationCall::CreateWorkItem(p) => {
                assert!(is_v7(&p.id.0) && is_v7(&p.idempotency_key.0));
                assert_eq!(
                    serde_json::to_value(&p.due_at).unwrap(),
                    "2026-08-13T23:02:03.123456Z"
                );
                assert!(p.scheduled_at.is_none() && p.source_link.is_none());
            }
            other => panic!("unexpected call {other:?}"),
        }
        let revisions: Vec<_> = calls
            .iter()
            .filter_map(|call| match call {
                TestMutationCall::CompleteWorkItem(p) => Some(p.expected_revision.as_str()),
                TestMutationCall::CancelWorkItem(p) => Some(p.expected_revision.as_str()),
                TestMutationCall::AcknowledgeSignal(p) => Some(p.expected_revision.as_str()),
                TestMutationCall::AcknowledgeFire(p) => Some(p.expected_revision.as_str()),
                TestMutationCall::SnoozeFire(p) => Some(p.expected_revision.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(revisions, ["7", "8", "9", "10", "11"]);
        for call in calls.iter() {
            match call {
                TestMutationCall::CompleteWorkItem(p) => assert!(is_v7(&p.idempotency_key.0)),
                TestMutationCall::CancelWorkItem(p) => assert!(is_v7(&p.idempotency_key.0)),
                TestMutationCall::AcknowledgeSignal(p) => assert!(is_v7(&p.idempotency_key.0)),
                TestMutationCall::CreateReminder(p) => {
                    assert!(
                        is_v7(&p.reminder_id.0)
                            && is_v7(&p.initial_fire_id.0)
                            && is_v7(&p.idempotency_key.0)
                    );
                    assert_eq!(
                        serde_json::to_value(&p.trigger_at).unwrap(),
                        "2026-08-13T23:02:03.123456Z"
                    );
                }
                TestMutationCall::AcknowledgeFire(p) => assert!(is_v7(&p.idempotency_key.0)),
                TestMutationCall::SnoozeFire(p) => {
                    assert!(is_v7(&p.replacement_fire_id.0) && is_v7(&p.idempotency_key.0));
                    assert_eq!(
                        serde_json::to_value(&p.replacement_trigger_at).unwrap(),
                        "2026-08-16T01:02:03.123456Z"
                    );
                }
                TestMutationCall::CreateWorkItem(_) => {}
            }
        }
    }

    #[test]
    fn mock_ipc_maps_allowlisted_conflicts_sanitizes_peer_text_and_preserves_ambiguity() {
        let expected =
            protocol::V1Error::ExpectedRevisionConflict(protocol::ExpectedRevisionConflictData {
                resource: protocol::ResourceRef::WorkItem {
                    id: protocol::WorkItemId("work".into()),
                },
                expected: protocol::Revision::parse("4").unwrap(),
                actual: protocol::Revision::parse("5").unwrap(),
            })
            .try_into()
            .unwrap();
        let create = protocol::V1Error::CreateConflict(protocol::CreateConflictData {
            resource: protocol::ResourceRef::Reminder {
                id: protocol::ReminderId("reminder".into()),
            },
        })
        .try_into()
        .unwrap();
        let generic = protocol::RpcError {
            code: protocol::ErrorCode(-32999),
            message: "secret peer canary".into(),
            data: Some(serde_json::json!({"secret":"canary"})),
        };
        let (supervisor, transport) = DesktopSupervisor::for_test(1, None, &[], VecDeque::new());
        transport.script([
            TestMutationResult::WorkItem(Err(ClientError::Peer(expected))),
            TestMutationResult::WorkItem(Err(ClientError::Peer(create))),
            TestMutationResult::WorkItem(Err(ClientError::Peer(generic))),
            TestMutationResult::WorkItem(Err(ClientError::AmbiguousMutation)),
        ]);
        let app = app_with(supervisor);
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let body =
            |revision: &str| serde_json::json!({"input":{"id":"work","expectedRevision":revision}});
        let revision = invoke(&webview, "desktop_complete_work_item", body("4")).unwrap_err();
        assert_eq!(
            revision,
            serde_json::json!({"category":"expected_revision_conflict","message":"resource revision changed","resourceKind":"work_item","resourceId":"work","expectedRevision":"4","actualRevision":"5"})
        );
        let conflict = invoke(&webview, "desktop_complete_work_item", body("4")).unwrap_err();
        assert_eq!(conflict["category"], "create_conflict");
        assert_eq!(conflict["resourceKind"], "reminder");
        assert_eq!(conflict["resourceId"], "reminder");
        let peer = invoke(&webview, "desktop_complete_work_item", body("4")).unwrap_err();
        assert_eq!(
            peer,
            serde_json::json!({"category":"peer","message":"server rejected the request"})
        );
        assert!(!peer.to_string().contains("secret"));
        let ambiguous = invoke(&webview, "desktop_complete_work_item", body("4")).unwrap_err();
        assert_eq!(ambiguous["category"], "ambiguous_mutation");
    }

    #[test]
    fn mock_ipc_bootstrap_is_atomic_and_commands_use_acl_identifiers() {
        let replay = VecDeque::from([
            DesktopMessageDto::Status {
                sequence: 1,
                generation: 7,
                status: ConnectionStatusDto::Connecting,
            },
            DesktopMessageDto::Reset {
                sequence: 2,
                generation: 7,
                reason: ResetReason::Gap,
            },
        ]);
        let (supervisor, _) = DesktopSupervisor::for_test(7, Some("snapshot-7"), &[], replay);
        let app = app_with(supervisor);
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock webview");

        let state = invoke(&webview, "desktop_state", serde_json::json!({})).expect("state IPC");
        assert_eq!(state["sequence"], 2);
        assert_eq!(state["generation"], 7);
        assert_eq!(state["snapshotAfterCursor"], "snapshot-7");
        assert_eq!(state["replay"][0]["sequence"], 1);
        assert_eq!(state["replay"][1]["sequence"], state["sequence"]);
        assert!(
            invoke(
                &webview,
                "unregistered_desktop_state",
                serde_json::json!({})
            )
            .is_err()
        );

        let capability = include_str!("../capabilities/default.json");
        for identifier in [
            "allow-desktop-state",
            "allow-desktop-acknowledge-snapshot",
            "allow-desktop-acknowledge-change",
        ] {
            assert!(
                capability.contains(identifier),
                "missing ACL identifier {identifier}"
            );
        }
    }

    #[test]
    fn mock_ipc_enforces_snapshot_then_strict_fifo_change_acknowledgements() {
        let (supervisor, transport) = DesktopSupervisor::for_test(
            3,
            Some("snapshot"),
            &["change-1", "change-2"],
            VecDeque::new(),
        );
        let app = app_with(supervisor);
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock webview");

        let change = |cursor: &str| serde_json::json!({ "generation": 3, "cursor": cursor });
        assert!(invoke(&webview, "desktop_acknowledge_change", change("change-1")).is_err());
        assert!(
            invoke(
                &webview,
                "desktop_acknowledge_snapshot",
                serde_json::json!({
                    "generation": 3, "afterCursor": "wrong"
                })
            )
            .is_err()
        );
        invoke(
            &webview,
            "desktop_acknowledge_snapshot",
            serde_json::json!({
                "generation": 3, "afterCursor": "snapshot"
            }),
        )
        .expect("snapshot ack");
        assert!(invoke(&webview, "desktop_acknowledge_change", change("change-2")).is_err());
        invoke(&webview, "desktop_acknowledge_change", change("change-1")).expect("first ack");
        assert!(invoke(&webview, "desktop_acknowledge_change", change("change-1")).is_err());
        invoke(&webview, "desktop_acknowledge_change", change("change-2")).expect("second ack");

        assert_eq!(
            *transport.acknowledgements.lock().expect("test ack log"),
            vec![
                (true, "snapshot".into()),
                (false, "change-1".into()),
                (false, "change-2".into())
            ]
        );
    }

    #[tokio::test]
    async fn change_is_applied_before_failed_ack_and_redelivery_remains_idempotent() {
        let event = updating_change("change-1");
        let replay = VecDeque::from([DesktopMessageDto::Change {
            sequence: 1,
            generation: 3,
            event: event.clone(),
        }]);
        let (supervisor, transport) = DesktopSupervisor::for_test(3, None, &["change-1"], replay);
        supervisor.set_snapshot_for_test(ordered_snapshot()).await;
        transport.script_acknowledgements([
            Err(ClientError::Transport("injected ack failure".into())),
            Ok(()),
        ]);

        let error = supervisor
            .acknowledge_change(3, "change-1".into())
            .await
            .expect_err("first acknowledgement must fail");
        assert_eq!(error.category, "transport");
        let applied = supervisor.state().await;
        let snapshot = applied.snapshot.expect("materialized snapshot");
        assert_eq!(snapshot.work_items[0].revision, "2");
        assert_eq!(snapshot.work_items[1].id, "second");
        assert!(applied.replay.is_empty());

        supervisor.redeliver_change_for_test(event).await;
        supervisor
            .acknowledge_change(3, "change-1".into())
            .await
            .expect("redelivered acknowledgement");
        let redelivered = supervisor.state().await.snapshot.expect("snapshot");
        assert_eq!(redelivered.work_items.len(), 2);
        assert_eq!(redelivered.work_items[0].revision, "2");
        assert_eq!(redelivered.work_items[1].id, "second");
        assert_eq!(
            *transport.acknowledgements.lock().expect("ack log"),
            vec![(false, "change-1".into()), (false, "change-1".into())]
        );
    }

    #[tokio::test]
    async fn stream_change_snapshot_restoration_allows_a_later_gap_reset() {
        let (supervisor, _) =
            DesktopSupervisor::for_test(1, Some("snapshot"), &[], VecDeque::new());
        supervisor.set_snapshot_for_test(ordered_snapshot()).await;
        supervisor
            .expect_identity_for_test(
                protocol::ServerId("old-server".into()),
                protocol::StreamId("old-stream".into()),
            )
            .await;

        let restored = supervisor
            .update_status_for_test(ConnectionStatus::Connected {
                server_id: protocol::ServerId("new-server".into()),
                stream_id: protocol::StreamId("new-stream".into()),
            })
            .await;
        assert!(matches!(restored, DesktopMessageDto::Snapshot { .. }));

        let gap = supervisor
            .update_status_for_test(ConnectionStatus::Gap)
            .await;
        assert!(matches!(
            gap,
            DesktopMessageDto::Reset {
                reason: ResetReason::Gap,
                ..
            }
        ));
    }

    #[test]
    fn mock_ipc_rejects_stale_generation_after_reset_and_replay_overflow() {
        let (supervisor, _) =
            DesktopSupervisor::for_test(9, Some("snapshot"), &["change"], VecDeque::new());
        tauri::async_runtime::block_on(supervisor.force_reset(ResetReason::Gap));
        tauri::async_runtime::block_on(supervisor.fill_replay_to_overflow());
        let app = app_with(supervisor);
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock webview");

        assert!(
            invoke(
                &webview,
                "desktop_acknowledge_snapshot",
                serde_json::json!({
                    "generation": 9, "afterCursor": "snapshot"
                })
            )
            .is_err()
        );
        assert!(
            invoke(
                &webview,
                "desktop_acknowledge_change",
                serde_json::json!({
                    "generation": 9, "cursor": "change"
                })
            )
            .is_err()
        );
        let state = invoke(&webview, "desktop_state", serde_json::json!({})).expect("state IPC");
        assert_eq!(state["generation"], 11);
        assert_eq!(state["replay"].as_array().expect("replay").len(), 1);
        assert_eq!(state["replay"][0]["type"], "reset");
        assert_eq!(state["replay"][0]["reason"], "overflow");
        assert!(state.get("snapshotAfterCursor").is_none());
    }

    #[test]
    fn production_initialization_enters_tauri_runtime_before_starting_client() {
        let app = mock_builder()
            .build(mock_context(tauri::test::noop_assets()))
            .expect("mock Tauri app");

        super::initialize_supervisor(app.handle(), "ws://127.0.0.1:9/v1/ws".to_string())
            .expect("production supervisor initialization must not panic without a Tokio context");

        assert!(app.handle().try_state::<DesktopSupervisor>().is_some());
        super::shutdown_supervisor(app.handle());
    }

    #[test]
    fn native_shutdown_closes_managed_supervisor_once() {
        let (supervisor, transport) = DesktopSupervisor::for_test(1, None, &[], VecDeque::new());
        let app = app_with(supervisor);
        super::shutdown_supervisor(app.handle());
        super::shutdown_supervisor(app.handle());
        assert!(transport.closed.load(Ordering::Acquire));
        assert_eq!(Arc::strong_count(&transport), 2);
    }

    #[test]
    fn presentation_omits_source_link() {
        let view = WorkItemView {
            id: WorkItemId("work-1".into()),
            revision: Revision::parse("1").expect("revision"),
            lifecycle: WorkItemLifecycle::Open,
            due_at: None,
            scheduled_at: None,
            defer_until: None,
            source_link: None,
        };
        let value = serde_json::to_value(WorkItemDto::from(view)).expect("serialize DTO");
        assert_eq!(value["id"], "work-1");
        assert!(value.get("sourceLink").is_none());
    }

    #[test]
    fn errors_are_sanitized() {
        let error = DesktopErrorDto::from(ClientError::Transport("secret-canary".into()));
        let json = serde_json::to_string(&error).expect("serialize error");
        assert!(!json.contains("secret-canary"));
        assert!(json.contains("transport"));
    }

    #[test]
    fn connected_status_hides_server_identity() {
        let json =
            serde_json::to_string(&ConnectionStatusDto::Connected).expect("serialize status");
        assert_eq!(json, r#"{"kind":"connected"}"#);
    }
}
