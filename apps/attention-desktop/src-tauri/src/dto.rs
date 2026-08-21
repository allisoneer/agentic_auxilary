use attention_client::ConnectionStatus;
use attention_protocol as protocol;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStateDto {
    pub sequence: u64,
    pub generation: u64,
    pub status: ConnectionStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<SnapshotDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_after_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<IssueDto>,
    pub replay: Vec<DesktopMessageDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectionStatusDto {
    Connecting,
    Connected,
    Reconnecting { attempt: u32 },
    Gap,
    Closed,
}

impl From<&ConnectionStatus> for ConnectionStatusDto {
    fn from(value: &ConnectionStatus) -> Self {
        match value {
            ConnectionStatus::Connecting => Self::Connecting,
            ConnectionStatus::Connected { .. } => Self::Connected,
            ConnectionStatus::Reconnecting { attempt } => Self::Reconnecting { attempt: *attempt },
            ConnectionStatus::Gap => Self::Gap,
            ConnectionStatus::Closed => Self::Closed,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDto {
    pub work_items: Vec<WorkItemDto>,
    pub attention_signals: Vec<AttentionSignalDto>,
    pub reminders: Vec<ReminderDto>,
    pub inbox: Vec<InboxEntryDto>,
}

impl From<protocol::AttentionSnapshot> for SnapshotDto {
    fn from(value: protocol::AttentionSnapshot) -> Self {
        Self {
            work_items: value.work_items.into_iter().map(Into::into).collect(),
            attention_signals: value
                .attention_signals
                .into_iter()
                .map(Into::into)
                .collect(),
            reminders: value.reminders.into_iter().map(Into::into).collect(),
            inbox: value.inbox.into_iter().map(Into::into).collect(),
        }
    }
}

impl SnapshotDto {
    pub(crate) fn apply(&mut self, event: &ChangeEventDto) {
        for affected in &event.affected {
            match affected {
                AffectedViewDto::WorkItem { work_item } => {
                    if let Some(index) = self
                        .work_items
                        .iter()
                        .position(|item| item.id == work_item.id)
                    {
                        self.work_items[index] = work_item.clone();
                    } else {
                        self.work_items.push(work_item.clone());
                    }
                }
                AffectedViewDto::AttentionSignal { attention_signal } => {
                    if let Some(index) = self
                        .attention_signals
                        .iter()
                        .position(|item| item.id == attention_signal.id)
                    {
                        self.attention_signals[index] = attention_signal.clone();
                    } else {
                        self.attention_signals.push(attention_signal.clone());
                    }
                }
                AffectedViewDto::Reminder { reminder } => {
                    if let Some(index) = self
                        .reminders
                        .iter()
                        .position(|item| item.id == reminder.id)
                    {
                        self.reminders[index] = reminder.clone();
                    } else {
                        self.reminders.push(reminder.clone());
                    }
                }
            }
        }
        self.inbox.retain(|entry| {
            !event
                .inbox
                .removals
                .iter()
                .any(|key| entry.matches_key(key))
        });
        for upsert in &event.inbox.upserts {
            if let Some(index) = self
                .inbox
                .iter()
                .position(|entry| entry.same_identity(upsert))
            {
                self.inbox[index] = upsert.clone();
            } else {
                self.inbox.push(upsert.clone());
            }
        }
    }
}

impl InboxEntryDto {
    fn same_identity(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::WorkItem { work_item: left }, Self::WorkItem { work_item: right }) => {
                left.id == right.id
            }
            (
                Self::AttentionSignal {
                    attention_signal: left,
                },
                Self::AttentionSignal {
                    attention_signal: right,
                },
            ) => left.id == right.id,
            (Self::ReminderFire { fire: left, .. }, Self::ReminderFire { fire: right, .. }) => {
                left.id == right.id
            }
            _ => false,
        }
    }

    fn matches_key(&self, key: &InboxKeyDto) -> bool {
        match (self, key) {
            (Self::WorkItem { work_item }, InboxKeyDto::WorkItem { work_item_id }) => {
                work_item.id == *work_item_id
            }
            (
                Self::AttentionSignal { attention_signal },
                InboxKeyDto::AttentionSignal {
                    attention_signal_id,
                },
            ) => attention_signal.id == *attention_signal_id,
            (Self::ReminderFire { fire, .. }, InboxKeyDto::ReminderFire { reminder_fire_id }) => {
                fire.id == *reminder_fire_id
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemDto {
    pub id: String,
    pub revision: String,
    pub lifecycle: protocol::WorkItemLifecycle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_at: Option<protocol::WireTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<protocol::WireTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_until: Option<protocol::WireTimestamp>,
}

impl From<protocol::WorkItemView> for WorkItemDto {
    fn from(value: protocol::WorkItemView) -> Self {
        Self {
            id: value.id.0,
            revision: value.revision.as_str().into(),
            lifecycle: value.lifecycle,
            due_at: value.due_at,
            scheduled_at: value.scheduled_at,
            defer_until: value.defer_until,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionSignalDto {
    pub id: String,
    pub revision: String,
    pub source_lifecycle: protocol::SignalSourceLifecycle,
    pub attention_state: protocol::SignalAttentionState,
}

impl From<protocol::AttentionSignalView> for AttentionSignalDto {
    fn from(value: protocol::AttentionSignalView) -> Self {
        Self {
            id: value.id.0,
            revision: value.revision.as_str().into(),
            source_lifecycle: value.source_lifecycle,
            attention_state: value.attention_state,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ReminderTargetDto {
    WorkItem { work_item_id: String },
    AttentionSignal { attention_signal_id: String },
}

impl From<protocol::ReminderTarget> for ReminderTargetDto {
    fn from(value: protocol::ReminderTarget) -> Self {
        match value {
            protocol::ReminderTarget::WorkItem { work_item_id } => Self::WorkItem {
                work_item_id: work_item_id.0,
            },
            protocol::ReminderTarget::AttentionSignal {
                attention_signal_id,
            } => Self::AttentionSignal {
                attention_signal_id: attention_signal_id.0,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderFireDto {
    pub id: String,
    pub trigger_at: protocol::WireTimestamp,
    pub state: protocol::ReminderFireState,
}

impl From<protocol::ReminderFireView> for ReminderFireDto {
    fn from(value: protocol::ReminderFireView) -> Self {
        Self {
            id: value.id.0,
            trigger_at: value.trigger_at,
            state: value.state,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderDto {
    pub id: String,
    pub revision: String,
    pub target: ReminderTargetDto,
    pub trigger_at: protocol::WireTimestamp,
    pub fires: Vec<ReminderFireDto>,
}

impl From<protocol::ReminderView> for ReminderDto {
    fn from(value: protocol::ReminderView) -> Self {
        Self {
            id: value.id.0,
            revision: value.revision.as_str().into(),
            target: value.target.into(),
            trigger_at: value.trigger_at,
            fires: value.fires.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum InboxEntryDto {
    WorkItem {
        work_item: WorkItemDto,
    },
    AttentionSignal {
        attention_signal: AttentionSignalDto,
    },
    ReminderFire {
        reminder_id: String,
        reminder_revision: String,
        target: ReminderTargetDto,
        fire: ReminderFireDto,
    },
}

impl From<protocol::InboxEntryView> for InboxEntryDto {
    fn from(value: protocol::InboxEntryView) -> Self {
        match value {
            protocol::InboxEntryView::WorkItem { work_item } => Self::WorkItem {
                work_item: work_item.into(),
            },
            protocol::InboxEntryView::AttentionSignal { attention_signal } => {
                Self::AttentionSignal {
                    attention_signal: attention_signal.into(),
                }
            }
            protocol::InboxEntryView::ReminderFire {
                reminder_id,
                reminder_revision,
                target,
                fire,
            } => Self::ReminderFire {
                reminder_id: reminder_id.0,
                reminder_revision: reminder_revision.as_str().into(),
                target: target.into(),
                fire: fire.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum InboxKeyDto {
    WorkItem { work_item_id: String },
    AttentionSignal { attention_signal_id: String },
    ReminderFire { reminder_fire_id: String },
}

impl From<protocol::InboxEntryKey> for InboxKeyDto {
    fn from(value: protocol::InboxEntryKey) -> Self {
        match value {
            protocol::InboxEntryKey::WorkItem { work_item_id } => Self::WorkItem {
                work_item_id: work_item_id.0,
            },
            protocol::InboxEntryKey::AttentionSignal {
                attention_signal_id,
            } => Self::AttentionSignal {
                attention_signal_id: attention_signal_id.0,
            },
            protocol::InboxEntryKey::ReminderFire { reminder_fire_id } => Self::ReminderFire {
                reminder_fire_id: reminder_fire_id.0,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum AffectedViewDto {
    WorkItem {
        work_item: WorkItemDto,
    },
    AttentionSignal {
        attention_signal: AttentionSignalDto,
    },
    Reminder {
        reminder: ReminderDto,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEventDto {
    pub id: String,
    pub cursor: String,
    pub occurred_at: protocol::WireTimestamp,
    pub kind: protocol::ChangeKind,
    pub affected: Vec<AffectedViewDto>,
    pub inbox: InboxEffectsDto,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboxEffectsDto {
    pub upserts: Vec<InboxEntryDto>,
    pub removals: Vec<InboxKeyDto>,
}

impl From<protocol::ChangeEvent> for ChangeEventDto {
    fn from(value: protocol::ChangeEvent) -> Self {
        let affected = value
            .affected
            .into_iter()
            .filter_map(|view| match view {
                protocol::AffectedView::WorkItem { work_item } => Some(AffectedViewDto::WorkItem {
                    work_item: work_item.into(),
                }),
                protocol::AffectedView::AttentionSignal { attention_signal } => {
                    Some(AffectedViewDto::AttentionSignal {
                        attention_signal: attention_signal.into(),
                    })
                }
                protocol::AffectedView::Reminder { reminder } => Some(AffectedViewDto::Reminder {
                    reminder: reminder.into(),
                }),
                protocol::AffectedView::SourceReceipt { .. }
                | protocol::AffectedView::SourceEntity { .. } => None,
            })
            .collect();
        Self {
            id: value.id.0,
            cursor: value.cursor.0,
            occurred_at: value.occurred_at,
            kind: value.kind,
            affected,
            inbox: InboxEffectsDto {
                upserts: value.inbox.upserts.into_iter().map(Into::into).collect(),
                removals: value.inbox.removals.into_iter().map(Into::into).collect(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IssueDto {
    pub category: &'static str,
    pub message: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DesktopMessageDto {
    Status {
        sequence: u64,
        generation: u64,
        status: ConnectionStatusDto,
    },
    Reset {
        sequence: u64,
        generation: u64,
        reason: ResetReason,
    },
    Snapshot {
        sequence: u64,
        generation: u64,
        state: SnapshotDto,
        after_cursor: String,
    },
    Change {
        sequence: u64,
        generation: u64,
        event: ChangeEventDto,
    },
    Issue {
        sequence: u64,
        generation: u64,
        issue: IssueDto,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetReason {
    Gap,
    StreamChanged,
    Overflow,
    EmissionFailed,
}

#[cfg(test)]
mod sanitization_tests {
    use super::*;
    use attention_client::ClientError;
    use serde_json::Value;
    use serde_json::json;

    const SOURCE_CANARY: &str = "CANARY_SOURCE_IDENTITY";
    const PROVIDER_CANARY: &str = "CANARY_PROVIDER_PAYLOAD";
    const DATABASE_CANARY: &str = "CANARY_DATABASE_DETAIL";
    const TOKEN_CANARY: &str = "CANARY_SECRET_TOKEN";
    const RAW_ERROR_CANARY: &str = "CANARY_RAW_ERROR";

    fn work_item(id: &str) -> Value {
        json!({
            "id": id,
            "revision": "7",
            "lifecycle": "open",
            "due_at": "2026-08-13T20:00:00.000000Z",
            "scheduled_at": "2026-08-13T21:00:00.000000Z",
            "defer_until": "2026-08-13T22:00:00.000000Z",
            "source_link": {
                "source_kind": SOURCE_CANARY,
                "source_instance": PROVIDER_CANARY,
                "external_entity_id": DATABASE_CANARY
            }
        })
    }

    fn signal(id: &str) -> Value {
        json!({
            "id": id,
            "revision": "8",
            "source_lifecycle": "active",
            "attention_state": "unread",
            "source_receipt_id": SOURCE_CANARY,
            "source_entity_id": PROVIDER_CANARY
        })
    }

    fn reminder(id: &str, target: &Value) -> Value {
        json!({
            "id": id,
            "revision": "9",
            "target": target,
            "trigger_at": "2026-08-14T01:00:00.000000Z",
            "fires": [{
                "id": format!("{id}-fire"),
                "trigger_at": "2026-08-14T01:00:00.000000Z",
                "state": "fired"
            }]
        })
    }

    fn source_receipt() -> Value {
        json!({
            "id": SOURCE_CANARY,
            "occurrence_key": {
                "source_kind": PROVIDER_CANARY,
                "source_instance": DATABASE_CANARY,
                "occurrence_id": TOKEN_CANARY
            },
            "source_entity_key": {
                "source_kind": PROVIDER_CANARY,
                "source_instance": DATABASE_CANARY,
                "external_entity_id": TOKEN_CANARY
            },
            "source_order": { "mode": "unordered" },
            "occurred_at": "2026-08-13T20:00:00.000000Z",
            "ingested_at": "2026-08-13T20:00:01.000000Z"
        })
    }

    fn source_entity() -> Value {
        json!({
            "id": SOURCE_CANARY,
            "key": {
                "source_kind": PROVIDER_CANARY,
                "source_instance": DATABASE_CANARY,
                "external_entity_id": TOKEN_CANARY
            },
            "version": "2",
            "latest_receipt_id": RAW_ERROR_CANARY,
            "order": { "mode": "unordered" }
        })
    }

    fn snapshot() -> protocol::AttentionSnapshot {
        serde_json::from_value(json!({
            "work_items": [work_item("work-present")],
            "attention_signals": [signal("signal-present")],
            "reminders": [
                reminder("reminder-work", &json!({"kind": "work_item", "work_item_id": "work-present"})),
                reminder("reminder-signal", &json!({"kind": "attention_signal", "attention_signal_id": "signal-present"}))
            ],
            "inbox": [
                {"kind": "work_item", "work_item": work_item("inbox-work")},
                {"kind": "attention_signal", "attention_signal": signal("inbox-signal")},
                {
                    "kind": "reminder_fire",
                    "reminder_id": "inbox-reminder-work",
                    "reminder_revision": "10",
                    "target": {"kind": "work_item", "work_item_id": "work-present"},
                    "fire": {"id": "inbox-fire-work", "trigger_at": "2026-08-14T02:00:00.000000Z", "state": "scheduled"}
                },
                {
                    "kind": "reminder_fire",
                    "reminder_id": "inbox-reminder-signal",
                    "reminder_revision": "11",
                    "target": {"kind": "attention_signal", "attention_signal_id": "signal-present"},
                    "fire": {"id": "inbox-fire-signal", "trigger_at": "2026-08-14T03:00:00.000000Z", "state": "snoozed"}
                }
            ]
        })).expect("complete protocol snapshot fixture")
    }

    fn change() -> protocol::ChangeEvent {
        serde_json::from_value(json!({
            "id": "change-present",
            "cursor": "cursor-present",
            "occurred_at": "2026-08-13T23:00:00.000000Z",
            "kind": "source_occurrence_ingested",
            "affected": [
                {"kind": "work_item", "work_item": work_item("affected-work")},
                {"kind": "attention_signal", "attention_signal": signal("affected-signal")},
                {"kind": "reminder", "reminder": reminder("affected-reminder-work", &json!({"kind": "work_item", "work_item_id": "affected-work"}))},
                {"kind": "reminder", "reminder": reminder("affected-reminder-signal", &json!({"kind": "attention_signal", "attention_signal_id": "affected-signal"}))},
                {"kind": "source_receipt", "source_receipt": source_receipt()},
                {"kind": "source_entity", "source_entity": source_entity()}
            ],
            "inbox": {
                "upserts": [
                    {"kind": "work_item", "work_item": work_item("upsert-work")},
                    {"kind": "attention_signal", "attention_signal": signal("upsert-signal")},
                    {
                        "kind": "reminder_fire",
                        "reminder_id": "upsert-reminder",
                        "reminder_revision": "12",
                        "target": {"kind": "attention_signal", "attention_signal_id": "upsert-signal"},
                        "fire": {"id": "upsert-fire", "trigger_at": "2026-08-14T04:00:00.000000Z", "state": "acknowledged"}
                    }
                ],
                "removals": [
                    {"kind": "work_item", "work_item_id": "removed-work"},
                    {"kind": "attention_signal", "attention_signal_id": "removed-signal"},
                    {"kind": "reminder_fire", "reminder_fire_id": "removed-fire"}
                ]
            }
        })).expect("complete protocol event fixture")
    }

    fn assert_ipc_safe(value: &Value) {
        let encoded = serde_json::to_string(value).expect("serialize JSON value");
        for canary in [
            SOURCE_CANARY,
            PROVIDER_CANARY,
            DATABASE_CANARY,
            TOKEN_CANARY,
            RAW_ERROR_CANARY,
        ] {
            assert!(
                !encoded.contains(canary),
                "forbidden canary crossed IPC: {canary}: {encoded}"
            );
        }
        assert_no_forbidden_keys(value);
    }

    fn assert_no_forbidden_keys(value: &Value) {
        const FORBIDDEN: &[&str] = &[
            "sourceLink",
            "source_link",
            "sourceReceiptId",
            "source_receipt_id",
            "sourceEntityId",
            "source_entity_id",
            "sourceReceipt",
            "source_receipt",
            "sourceEntity",
            "source_entity",
            "occurrenceKey",
            "occurrence_key",
            "sourceEntityKey",
            "source_entity_key",
            "sourceOrder",
            "source_order",
            "provider",
            "providerId",
            "providerMessageId",
            "database",
            "databaseUrl",
            "token",
            "accessToken",
            "refreshToken",
            "rawError",
            "raw_error",
        ];
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    assert!(
                        !FORBIDDEN.contains(&key.as_str()),
                        "forbidden IPC field {key}"
                    );
                    assert_no_forbidden_keys(child);
                }
            }
            Value::Array(values) => values.iter().for_each(assert_no_forbidden_keys),
            _ => {}
        }
    }

    fn assert_client_error_data_is_dropped(value: &Value) {
        match value {
            Value::Object(object) => {
                assert!(
                    !object.contains_key("data"),
                    "client error data crossed IPC"
                );
                object
                    .values()
                    .for_each(assert_client_error_data_is_dropped);
            }
            Value::Array(values) => values.iter().for_each(assert_client_error_data_is_dropped),
            _ => {}
        }
    }

    fn issue(category: &'static str, message: &'static str) -> IssueDto {
        IssueDto { category, message }
    }

    #[test]
    fn exhaustive_snapshot_and_event_branches_are_recursively_sanitized() {
        let snapshot = SnapshotDto::from(snapshot());
        let event = ChangeEventDto::from(change());
        let snapshot_json = serde_json::to_value(&snapshot).expect("serialize snapshot DTO");
        let event_json = serde_json::to_value(&event).expect("serialize event DTO");
        assert_ipc_safe(&snapshot_json);
        assert_ipc_safe(&event_json);

        assert_eq!(snapshot_json["workItems"][0]["id"], "work-present");
        assert_eq!(snapshot_json["attentionSignals"][0]["id"], "signal-present");
        assert_eq!(snapshot_json["reminders"].as_array().map(Vec::len), Some(2));
        assert_eq!(snapshot_json["inbox"].as_array().map(Vec::len), Some(4));
        assert_eq!(event_json["id"], "change-present");
        assert_eq!(
            event_json["affected"].as_array().map(Vec::len),
            Some(4),
            "two private source affected views must be dropped"
        );
        assert_eq!(
            event_json["inbox"]["upserts"].as_array().map(Vec::len),
            Some(3)
        );
        assert_eq!(
            event_json["inbox"]["removals"].as_array().map(Vec::len),
            Some(3)
        );
    }

    #[test]
    fn snapshot_upserts_replace_existing_entries_without_reordering() {
        let mut snapshot = SnapshotDto::from(snapshot());
        let second_work = WorkItemDto {
            id: "work-second".into(),
            revision: "1".into(),
            lifecycle: protocol::WorkItemLifecycle::Open,
            due_at: None,
            scheduled_at: None,
            defer_until: None,
        };
        snapshot.work_items.push(second_work.clone());
        let mut updated_work = snapshot.work_items[0].clone();
        updated_work.revision = "10".into();

        let second_inbox = InboxEntryDto::WorkItem {
            work_item: second_work,
        };
        snapshot.inbox.push(second_inbox);
        let mut updated_inbox = snapshot.inbox[0].clone();
        let InboxEntryDto::WorkItem { work_item } = &mut updated_inbox else {
            panic!("first inbox fixture must be a work item");
        };
        work_item.revision = "11".into();

        let mut event = ChangeEventDto::from(change());
        event.affected = vec![AffectedViewDto::WorkItem {
            work_item: updated_work,
        }];
        event.inbox = InboxEffectsDto {
            upserts: vec![updated_inbox],
            removals: vec![],
        };
        snapshot.apply(&event);

        assert_eq!(snapshot.work_items[0].revision, "10");
        assert_eq!(snapshot.work_items[1].id, "work-second");
        assert!(matches!(
            &snapshot.inbox[0],
            InboxEntryDto::WorkItem { work_item } if work_item.revision == "11"
        ));
        assert!(matches!(
            &snapshot.inbox[4],
            InboxEntryDto::WorkItem { work_item } if work_item.id == "work-second"
        ));
    }

    #[test]
    fn every_desktop_message_and_state_issue_path_is_sanitized() {
        let snapshot = SnapshotDto::from(snapshot());
        let event = ChangeEventDto::from(change());
        let messages = vec![
            DesktopMessageDto::Status {
                sequence: 1,
                generation: 1,
                status: ConnectionStatusDto::Connected,
            },
            DesktopMessageDto::Reset {
                sequence: 2,
                generation: 2,
                reason: ResetReason::Gap,
            },
            DesktopMessageDto::Snapshot {
                sequence: 3,
                generation: 2,
                state: snapshot.clone(),
                after_cursor: "required-after-cursor".into(),
            },
            DesktopMessageDto::Change {
                sequence: 4,
                generation: 2,
                event,
            },
            DesktopMessageDto::Issue {
                sequence: 5,
                generation: 2,
                issue: issue("transport", "desktop connection failed"),
            },
        ];
        for message in &messages {
            assert_ipc_safe(&serde_json::to_value(message).expect("serialize message DTO"));
        }
        let state = DesktopStateDto {
            sequence: 5,
            generation: 2,
            status: ConnectionStatusDto::Reconnecting { attempt: 3 },
            snapshot: Some(snapshot),
            snapshot_after_cursor: Some("required-after-cursor".into()),
            issue: Some(issue("peer", "server rejected the request")),
            replay: messages,
        };
        let value = serde_json::to_value(state).expect("serialize state DTO");
        assert_ipc_safe(&value);
        assert_eq!(value["sequence"], 5);
        assert_eq!(value["issue"]["category"], "peer");
        assert_eq!(value["replay"].as_array().map(Vec::len), Some(5));
    }

    #[test]
    fn every_client_error_variant_and_issue_projection_drops_raw_details() {
        let errors = [
            ClientError::Peer(protocol::RpcError {
                code: protocol::ErrorCode(-39999),
                message: RAW_ERROR_CANARY.into(),
                data: Some(json!({
                    "occurrence_key": SOURCE_CANARY,
                    "provider": PROVIDER_CANARY,
                    "database": DATABASE_CANARY,
                    "token": TOKEN_CANARY
                })),
            }),
            ClientError::Transport(RAW_ERROR_CANARY.into()),
            ClientError::Timeout,
            ClientError::LocalProtocol(TOKEN_CANARY.into()),
            ClientError::Backpressure(PROVIDER_CANARY),
            ClientError::AmbiguousMutation,
            ClientError::InvalidCursorAcknowledgement(DATABASE_CANARY),
            ClientError::Closed,
            ClientError::Configuration(SOURCE_CANARY),
        ];
        let expected = [
            "peer",
            "transport",
            "timeout",
            "transport",
            "backpressure",
            "ambiguous_mutation",
            "invalid_cursor_acknowledgement",
            "closed",
            "configuration",
        ];
        for (error, category) in errors.into_iter().zip(expected) {
            let mapped = crate::error::DesktopErrorDto::from(error);
            let error_value = serde_json::to_value(&mapped).expect("serialize command error DTO");
            assert_ipc_safe(&error_value);
            assert_client_error_data_is_dropped(&error_value);
            assert_eq!(error_value["category"], category);
            let issue_value = serde_json::to_value(DesktopMessageDto::Issue {
                sequence: 1,
                generation: 1,
                issue: issue(mapped.category, mapped.message),
            })
            .expect("serialize issue message DTO");
            assert_ipc_safe(&issue_value);
            assert_eq!(issue_value["issue"]["category"], category);
        }
    }
}
