use crate::error::DesktopErrorDto;
use attention_protocol as p;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkItemInput {
    pub due_at: Option<String>,
    pub scheduled_at: Option<String>,
    pub defer_until: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExistingWorkItemInput {
    pub id: String,
    pub expected_revision: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcknowledgeSignalInput {
    pub id: String,
    pub expected_revision: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReminderTargetInput {
    WorkItem { work_item_id: String },
    AttentionSignal { attention_signal_id: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateReminderInput {
    pub target: ReminderTargetInput,
    pub trigger_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcknowledgeFireInput {
    pub reminder_id: String,
    pub fire_id: String,
    pub expected_revision: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnoozeFireInput {
    pub reminder_id: String,
    pub fire_id: String,
    pub expected_revision: String,
    pub replacement_trigger_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationDispositionDto {
    Applied,
    Replayed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum MutationResourceDto {
    WorkItem {
        id: String,
    },
    AttentionSignal {
        id: String,
    },
    ReminderFire {
        reminder_id: String,
        fire_id: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationReceiptDto {
    pub disposition: MutationDispositionDto,
    pub cursor: String,
    pub change_event_id: String,
    pub resource: MutationResourceDto,
}

fn generated() -> String {
    Uuid::now_v7().to_string()
}
fn timestamp(value: &str) -> Result<p::WireTimestamp, DesktopErrorDto> {
    let parsed = DateTime::parse_from_rfc3339(value).map_err(|_| DesktopErrorDto::validation())?;
    let utc: DateTime<Utc> = parsed.with_timezone(&Utc);
    p::WireTimestamp::from_datetime(utc).map_err(|_| DesktopErrorDto::validation())
}
fn optional_timestamp(value: Option<&str>) -> Result<Option<p::WireTimestamp>, DesktopErrorDto> {
    value.map(timestamp).transpose()
}
fn revision(value: &str) -> Result<p::Revision, DesktopErrorDto> {
    p::Revision::parse(value).map_err(|_| DesktopErrorDto::validation())
}
fn disposition(value: p::MutationDisposition) -> MutationDispositionDto {
    match value {
        p::MutationDisposition::Applied => MutationDispositionDto::Applied,
        p::MutationDisposition::Replayed => MutationDispositionDto::Replayed,
    }
}

pub fn create_work_item_params(
    input: &CreateWorkItemInput,
) -> Result<p::CreateWorkItemParams, DesktopErrorDto> {
    Ok(p::CreateWorkItemParams {
        id: p::WorkItemId(generated()),
        due_at: optional_timestamp(input.due_at.as_deref())?,
        scheduled_at: optional_timestamp(input.scheduled_at.as_deref())?,
        defer_until: optional_timestamp(input.defer_until.as_deref())?,
        source_link: None,
        idempotency_key: p::MutationIdempotencyKey(generated()),
    })
}
pub fn complete_work_item_params(
    input: ExistingWorkItemInput,
) -> Result<p::CompleteWorkItemParams, DesktopErrorDto> {
    Ok(p::CompleteWorkItemParams {
        id: p::WorkItemId(input.id),
        expected_revision: revision(&input.expected_revision)?,
        idempotency_key: p::MutationIdempotencyKey(generated()),
    })
}
pub fn cancel_work_item_params(
    input: ExistingWorkItemInput,
) -> Result<p::CancelWorkItemParams, DesktopErrorDto> {
    Ok(p::CancelWorkItemParams {
        id: p::WorkItemId(input.id),
        expected_revision: revision(&input.expected_revision)?,
        idempotency_key: p::MutationIdempotencyKey(generated()),
    })
}
pub fn acknowledge_signal_params(
    input: AcknowledgeSignalInput,
) -> Result<p::AcknowledgeAttentionSignalParams, DesktopErrorDto> {
    Ok(p::AcknowledgeAttentionSignalParams {
        id: p::AttentionSignalId(input.id),
        expected_revision: revision(&input.expected_revision)?,
        idempotency_key: p::MutationIdempotencyKey(generated()),
    })
}
pub fn create_reminder_params(
    input: CreateReminderInput,
) -> Result<p::CreateReminderParams, DesktopErrorDto> {
    let target = match input.target {
        ReminderTargetInput::WorkItem { work_item_id } => p::ReminderTarget::WorkItem {
            work_item_id: p::WorkItemId(work_item_id),
        },
        ReminderTargetInput::AttentionSignal {
            attention_signal_id,
        } => p::ReminderTarget::AttentionSignal {
            attention_signal_id: p::AttentionSignalId(attention_signal_id),
        },
    };
    Ok(p::CreateReminderParams {
        reminder_id: p::ReminderId(generated()),
        initial_fire_id: p::ReminderFireId(generated()),
        target,
        trigger_at: timestamp(&input.trigger_at)?,
        idempotency_key: p::MutationIdempotencyKey(generated()),
    })
}
pub fn acknowledge_fire_params(
    input: AcknowledgeFireInput,
) -> Result<p::AcknowledgeReminderFireParams, DesktopErrorDto> {
    Ok(p::AcknowledgeReminderFireParams {
        reminder_id: p::ReminderId(input.reminder_id),
        fire_id: p::ReminderFireId(input.fire_id),
        expected_revision: revision(&input.expected_revision)?,
        idempotency_key: p::MutationIdempotencyKey(generated()),
    })
}
pub fn snooze_fire_params(
    input: SnoozeFireInput,
) -> Result<p::SnoozeReminderFireParams, DesktopErrorDto> {
    Ok(p::SnoozeReminderFireParams {
        reminder_id: p::ReminderId(input.reminder_id),
        fire_id: p::ReminderFireId(input.fire_id),
        replacement_fire_id: p::ReminderFireId(generated()),
        replacement_trigger_at: timestamp(&input.replacement_trigger_at)?,
        expected_revision: revision(&input.expected_revision)?,
        idempotency_key: p::MutationIdempotencyKey(generated()),
    })
}

pub fn work_item_receipt(
    result: p::MutationResult<p::WorkItemMutationValue>,
) -> MutationReceiptDto {
    MutationReceiptDto {
        disposition: disposition(result.disposition),
        cursor: result.cursor.0,
        change_event_id: result.change_event_id.0,
        resource: MutationResourceDto::WorkItem {
            id: result.value.id.0,
        },
    }
}
pub fn signal_receipt(
    result: p::MutationResult<p::AttentionSignalMutationValue>,
) -> MutationReceiptDto {
    MutationReceiptDto {
        disposition: disposition(result.disposition),
        cursor: result.cursor.0,
        change_event_id: result.change_event_id.0,
        resource: MutationResourceDto::AttentionSignal {
            id: result.value.id.0,
        },
    }
}
pub fn reminder_receipt(result: p::MutationResult<p::ReminderMutationValue>) -> MutationReceiptDto {
    MutationReceiptDto {
        disposition: disposition(result.disposition),
        cursor: result.cursor.0,
        change_event_id: result.change_event_id.0,
        resource: MutationResourceDto::ReminderFire {
            reminder_id: result.value.reminder_id.0,
            fire_id: result.value.fire_id.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn is_v7(value: &str) -> bool {
        Uuid::parse_str(value).is_ok_and(|id| id.get_version_num() == 7)
    }
    #[test]
    fn params_use_canonical_utc_v7_ids_and_preserve_revision() {
        let work = create_work_item_params(&CreateWorkItemInput {
            due_at: Some("2026-08-14T01:02:03.123456+02:00".into()),
            scheduled_at: None,
            defer_until: None,
        })
        .unwrap();
        assert!(is_v7(&work.id.0) && is_v7(&work.idempotency_key.0));
        assert_eq!(
            serde_json::to_value(work.due_at).unwrap(),
            "2026-08-13T23:02:03.123456Z"
        );
        assert!(work.source_link.is_none());
        let snooze = snooze_fire_params(SnoozeFireInput {
            reminder_id: "r".into(),
            fire_id: "f".into(),
            expected_revision: "42".into(),
            replacement_trigger_at: "2026-08-14T01:02:03.123456Z".into(),
        })
        .unwrap();
        assert_eq!(snooze.expected_revision.as_str(), "42");
        assert!(is_v7(&snooze.replacement_fire_id.0) && is_v7(&snooze.idempotency_key.0));
    }
    #[test]
    fn invalid_time_and_revision_are_sanitized_validation() {
        assert_eq!(timestamp("local-time").unwrap_err().category, "validation");
        assert_eq!(
            timestamp("2026-08-14T01:02:03.123456789Z")
                .unwrap_err()
                .category,
            "validation"
        );
        assert_eq!(revision("01").unwrap_err().category, "validation");
    }
    #[test]
    fn receipt_uses_camel_case_resource_fields_and_drops_outbox() {
        let receipt = reminder_receipt(p::MutationResult {
            disposition: p::MutationDisposition::Applied,
            value: p::ReminderMutationValue {
                reminder_id: p::ReminderId("r".into()),
                fire_id: p::ReminderFireId("f".into()),
            },
            cursor: p::Cursor("c".into()),
            change_event_id: p::ChangeEventId("e".into()),
            outbox_intent_id: Some(p::OutboxIntentId("private-outbox".into())),
        });
        let json = serde_json::to_value(&receipt).unwrap();
        assert_eq!(json["resource"]["reminderId"], "r");
        assert_eq!(json["resource"]["fireId"], "f");
        assert!(json["resource"].get("reminder_id").is_none());
        assert!(json["resource"].get("fire_id").is_none());
        assert!(!json.to_string().contains("outbox"));
    }
}
