use crate::Error;
use attention_kernel::AttentionSignal;
use attention_kernel::CanonicalFingerprint;
use attention_kernel::ChangeKind;
use attention_kernel::ExternalEntityId;
use attention_kernel::InvariantError;
use attention_kernel::MutationOperation;
use attention_kernel::NormalizedSourceOrder;
use attention_kernel::OccurrenceId;
use attention_kernel::OccurrenceKey;
use attention_kernel::Reminder;
use attention_kernel::ReminderFire;
use attention_kernel::ReminderFireId;
use attention_kernel::ReminderFireState;
use attention_kernel::ReminderId;
use attention_kernel::ReminderTarget;
use attention_kernel::Revision;
use attention_kernel::SignalAttentionState;
use attention_kernel::SignalSourceLifecycle;
use attention_kernel::SourceComparatorDomain;
use attention_kernel::SourceEntity;
use attention_kernel::SourceEntityKey;
use attention_kernel::SourceInstance;
use attention_kernel::SourceKind;
use attention_kernel::SourceOrderMode;
use attention_kernel::SourceReceipt;
use attention_kernel::SourceStateVersion;
use attention_kernel::WorkItem;
use attention_kernel::WorkItemLifecycle;
use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use std::fmt::Display;
use std::io;
use std::str::FromStr;
use turso_db::Row;

fn invalid(message: &'static str) -> Error {
    Error::Decode(Box::new(io::Error::new(
        io::ErrorKind::InvalidData,
        message,
    )))
}

pub fn id<T: Display>(value: T) -> String {
    value.to_string()
}

pub fn parse_id<T>(value: &str) -> Result<T, Error>
where
    T: FromStr<Err = InvariantError>,
{
    value
        .parse()
        .map_err(|error| Error::Decode(Box::new(error)))
}

pub fn counter(value: u64) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

pub fn parse_counter(bytes: &[u8]) -> Result<u64, Error> {
    let bytes: [u8; 8] = bytes
        .try_into()
        .map_err(|_| invalid("counter width is not eight"))?;
    let value = u64::from_be_bytes(bytes);
    if value == 0 {
        return Err(invalid("counter is zero"));
    }
    Ok(value)
}

pub fn timestamp(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

pub fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, Error> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| invalid("timestamp is malformed"))?
        .with_timezone(&Utc);
    if timestamp(&parsed) != value {
        return Err(invalid("timestamp is noncanonical"));
    }
    Ok(parsed)
}

pub fn fingerprint(value: CanonicalFingerprint) -> Vec<u8> {
    value.as_bytes().to_vec()
}

pub fn parse_fingerprint(bytes: &[u8]) -> Result<CanonicalFingerprint, Error> {
    let bytes = bytes
        .try_into()
        .map_err(|_| invalid("fingerprint width is not thirty-two"))?;
    Ok(CanonicalFingerprint::reconstruct(bytes))
}

pub fn operation(value: MutationOperation) -> i64 {
    match value {
        MutationOperation::CreateWorkItem => 0,
        MutationOperation::CompleteWorkItem => 1,
        MutationOperation::CancelWorkItem => 2,
        MutationOperation::AcknowledgeAttentionSignal => 3,
        MutationOperation::IngestSourceOccurrence => 4,
        MutationOperation::CreateReminder => 5,
        MutationOperation::FireReminder => 6,
        MutationOperation::AcknowledgeReminderFire => 7,
        MutationOperation::SnoozeReminderFire => 8,
    }
}

pub fn parse_operation(value: i64) -> Result<MutationOperation, Error> {
    match value {
        0 => Ok(MutationOperation::CreateWorkItem),
        1 => Ok(MutationOperation::CompleteWorkItem),
        2 => Ok(MutationOperation::CancelWorkItem),
        3 => Ok(MutationOperation::AcknowledgeAttentionSignal),
        4 => Ok(MutationOperation::IngestSourceOccurrence),
        5 => Ok(MutationOperation::CreateReminder),
        6 => Ok(MutationOperation::FireReminder),
        7 => Ok(MutationOperation::AcknowledgeReminderFire),
        8 => Ok(MutationOperation::SnoozeReminderFire),
        _ => Err(invalid("mutation operation is unknown")),
    }
}

pub fn change_kind(value: ChangeKind) -> i64 {
    match value {
        ChangeKind::WorkItemCreated => 0,
        ChangeKind::WorkItemCompleted => 1,
        ChangeKind::WorkItemCancelled => 2,
        ChangeKind::AttentionSignalAcknowledged => 3,
        ChangeKind::SourceOccurrenceIngested => 4,
        ChangeKind::ReminderCreated => 5,
        ChangeKind::ReminderFired => 6,
        ChangeKind::ReminderFireAcknowledged => 7,
        ChangeKind::ReminderFireSnoozed => 8,
    }
}

pub fn work_item_lifecycle(value: WorkItemLifecycle) -> i64 {
    match value {
        WorkItemLifecycle::Open => 0,
        WorkItemLifecycle::Completed => 1,
        WorkItemLifecycle::Cancelled => 2,
    }
}

pub fn signal_source_lifecycle(value: SignalSourceLifecycle) -> i64 {
    match value {
        SignalSourceLifecycle::Active => 0,
        SignalSourceLifecycle::Resolved => 1,
        SignalSourceLifecycle::Expired => 2,
    }
}

pub fn signal_attention_state(value: SignalAttentionState) -> i64 {
    match value {
        SignalAttentionState::Unread => 0,
        SignalAttentionState::Acknowledged => 1,
    }
}

pub fn reminder_fire_state(value: ReminderFireState) -> i64 {
    match value {
        ReminderFireState::Scheduled => 0,
        ReminderFireState::Fired => 1,
        ReminderFireState::Acknowledged => 2,
        ReminderFireState::Snoozed => 3,
    }
}

pub fn parse_change_kind(value: i64) -> Result<ChangeKind, Error> {
    match value {
        0 => Ok(ChangeKind::WorkItemCreated),
        1 => Ok(ChangeKind::WorkItemCompleted),
        2 => Ok(ChangeKind::WorkItemCancelled),
        3 => Ok(ChangeKind::AttentionSignalAcknowledged),
        4 => Ok(ChangeKind::SourceOccurrenceIngested),
        5 => Ok(ChangeKind::ReminderCreated),
        6 => Ok(ChangeKind::ReminderFired),
        7 => Ok(ChangeKind::ReminderFireAcknowledged),
        8 => Ok(ChangeKind::ReminderFireSnoozed),
        _ => Err(invalid("change kind is unknown")),
    }
}

pub fn source_order(value: &SourceOrderMode) -> (i64, Option<String>, Option<Vec<u8>>) {
    match value {
        SourceOrderMode::Unordered => (0, None, None),
        SourceOrderMode::Ordered { domain, value } => (
            1,
            Some(domain.as_str().to_string()),
            value.as_ref().map(|value| value.as_bytes().to_vec()),
        ),
    }
}

pub fn parse_source_order(
    mode: i64,
    domain: Option<String>,
    value: Option<Vec<u8>>,
) -> Result<SourceOrderMode, Error> {
    match (mode, domain, value) {
        (0, None, None) => Ok(SourceOrderMode::Unordered),
        (1, Some(domain), value) => Ok(SourceOrderMode::Ordered {
            domain: SourceComparatorDomain::new(domain)
                .map_err(|error| Error::Decode(Box::new(error)))?,
            value: value
                .map(NormalizedSourceOrder::new)
                .transpose()
                .map_err(|error| Error::Decode(Box::new(error)))?,
        }),
        _ => Err(invalid("source order columns are inconsistent")),
    }
}

fn optional_text(row: &Row, index: usize) -> Result<Option<String>, Error> {
    row.get(index)
        .map_err(|error| Error::Decode(Box::new(error)))
}

fn text(row: &Row, index: usize) -> Result<String, Error> {
    crate::decode::text(row, index)
}

fn integer(row: &Row, index: usize) -> Result<i64, Error> {
    crate::decode::integer(row, index)
}

fn blob(row: &Row, index: usize) -> Result<Vec<u8>, Error> {
    crate::decode::blob(row, index)
}

fn optional_key(
    kind: Option<String>,
    instance: Option<String>,
    external: Option<String>,
) -> Result<Option<SourceEntityKey>, Error> {
    match (kind, instance, external) {
        (None, None, None) => Ok(None),
        (Some(kind), Some(instance), Some(external)) => Ok(Some(SourceEntityKey::new(
            SourceKind::new(kind).map_err(|error| Error::Decode(Box::new(error)))?,
            SourceInstance::new(instance).map_err(|error| Error::Decode(Box::new(error)))?,
            ExternalEntityId::new(external).map_err(|error| Error::Decode(Box::new(error)))?,
        ))),
        _ => Err(invalid("optional source entity key is partial")),
    }
}

pub fn work_item(row: &Row) -> Result<WorkItem, Error> {
    let lifecycle = match integer(row, 2)? {
        0 => WorkItemLifecycle::Open,
        1 => WorkItemLifecycle::Completed,
        2 => WorkItemLifecycle::Cancelled,
        _ => return Err(invalid("work item lifecycle is unknown")),
    };
    WorkItem::reconstruct(
        parse_id(&text(row, 0)?)?,
        Revision::try_from(parse_counter(&blob(row, 1)?)?)
            .map_err(|error| Error::Decode(Box::new(error)))?,
        lifecycle,
        optional_text(row, 3)?
            .as_deref()
            .map(parse_timestamp)
            .transpose()?,
        optional_text(row, 4)?
            .as_deref()
            .map(parse_timestamp)
            .transpose()?,
        optional_text(row, 5)?
            .as_deref()
            .map(parse_timestamp)
            .transpose()?,
        optional_key(
            optional_text(row, 6)?,
            optional_text(row, 7)?,
            optional_text(row, 8)?,
        )?,
    )
    .map_err(|error| Error::Decode(Box::new(error)))
}

pub fn signal(row: &Row) -> Result<AttentionSignal, Error> {
    let source_lifecycle = match integer(row, 2)? {
        0 => SignalSourceLifecycle::Active,
        1 => SignalSourceLifecycle::Resolved,
        2 => SignalSourceLifecycle::Expired,
        _ => return Err(invalid("signal source lifecycle is unknown")),
    };
    let attention_state = match integer(row, 3)? {
        0 => SignalAttentionState::Unread,
        1 => SignalAttentionState::Acknowledged,
        _ => return Err(invalid("signal attention state is unknown")),
    };
    AttentionSignal::reconstruct(
        parse_id(&text(row, 0)?)?,
        Revision::try_from(parse_counter(&blob(row, 1)?)?)
            .map_err(|error| Error::Decode(Box::new(error)))?,
        source_lifecycle,
        attention_state,
        parse_id(&text(row, 4)?)?,
        optional_text(row, 5)?
            .as_deref()
            .map(parse_id)
            .transpose()?,
    )
    .map_err(|error| Error::Decode(Box::new(error)))
}

pub fn receipt(row: &Row) -> Result<SourceReceipt, Error> {
    let occurrence = OccurrenceKey::new(
        SourceKind::new(text(row, 1)?).map_err(|error| Error::Decode(Box::new(error)))?,
        SourceInstance::new(text(row, 2)?).map_err(|error| Error::Decode(Box::new(error)))?,
        OccurrenceId::new(text(row, 3)?).map_err(|error| Error::Decode(Box::new(error)))?,
    );
    SourceReceipt::reconstruct(
        parse_id(&text(row, 0)?)?,
        occurrence,
        optional_key(
            optional_text(row, 4)?,
            optional_text(row, 5)?,
            optional_text(row, 6)?,
        )?,
        parse_fingerprint(&blob(row, 7)?)?,
        parse_source_order(
            integer(row, 8)?,
            optional_text(row, 9)?,
            row.get(10)
                .map_err(|error| Error::Decode(Box::new(error)))?,
        )?,
        parse_timestamp(&text(row, 11)?)?,
        parse_timestamp(&text(row, 12)?)?,
    )
    .map_err(|error| Error::Decode(Box::new(error)))
}

pub fn entity(row: &Row) -> Result<SourceEntity, Error> {
    SourceEntity::reconstruct(
        parse_id(&text(row, 0)?)?,
        SourceEntityKey::new(
            SourceKind::new(text(row, 1)?).map_err(|error| Error::Decode(Box::new(error)))?,
            SourceInstance::new(text(row, 2)?).map_err(|error| Error::Decode(Box::new(error)))?,
            ExternalEntityId::new(text(row, 3)?).map_err(|error| Error::Decode(Box::new(error)))?,
        ),
        SourceStateVersion::try_from(parse_counter(&blob(row, 4)?)?)
            .map_err(|error| Error::Decode(Box::new(error)))?,
        parse_id(&text(row, 5)?)?,
        parse_source_order(
            integer(row, 6)?,
            optional_text(row, 7)?,
            row.get(8).map_err(|error| Error::Decode(Box::new(error)))?,
        )?,
    )
    .map_err(|error| Error::Decode(Box::new(error)))
}

pub struct ReminderHeader {
    pub id: ReminderId,
    pub revision: Revision,
    pub target: ReminderTarget,
    pub trigger_at: DateTime<Utc>,
    pub current_fire_id: Option<ReminderFireId>,
}

pub fn reminder_header(row: &Row) -> Result<ReminderHeader, Error> {
    let target = match integer(row, 2)? {
        0 => ReminderTarget::WorkItem(parse_id(&text(row, 3)?)?),
        1 => ReminderTarget::AttentionSignal(parse_id(&text(row, 3)?)?),
        _ => return Err(invalid("reminder target kind is unknown")),
    };
    Ok(ReminderHeader {
        id: parse_id(&text(row, 0)?)?,
        revision: Revision::try_from(parse_counter(&blob(row, 1)?)?)
            .map_err(|error| Error::Decode(Box::new(error)))?,
        target,
        trigger_at: parse_timestamp(&text(row, 4)?)?,
        current_fire_id: optional_text(row, 5)?
            .as_deref()
            .map(parse_id)
            .transpose()?,
    })
}

pub fn reminder_fire(row: &Row) -> Result<ReminderFire, Error> {
    let state = match integer(row, 2)? {
        0 => ReminderFireState::Scheduled,
        1 => ReminderFireState::Fired,
        2 => ReminderFireState::Acknowledged,
        3 => ReminderFireState::Snoozed,
        _ => return Err(invalid("reminder fire state is unknown")),
    };
    ReminderFire::reconstruct(
        parse_id(&text(row, 0)?)?,
        parse_timestamp(&text(row, 1)?)?,
        state,
    )
    .map_err(|error| Error::Decode(Box::new(error)))
}

pub fn reminder(header: &ReminderHeader, fires: Vec<ReminderFire>) -> Result<Reminder, Error> {
    let derived_current = fires
        .iter()
        .find(|fire| {
            matches!(
                fire.state(),
                ReminderFireState::Scheduled | ReminderFireState::Fired
            )
        })
        .map(ReminderFire::id);
    if derived_current != header.current_fire_id {
        return Err(invalid("reminder current fire pointer is inconsistent"));
    }
    Reminder::reconstruct(
        header.id,
        header.revision,
        header.target,
        header.trigger_at,
        fires,
    )
    .map_err(|error| Error::Decode(Box::new(error)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_times_fingerprints_and_closed_enums_fail_closed() {
        for value in [1, 255, 256, i64::MAX as u64 + 1, u64::MAX] {
            assert_eq!(parse_counter(&counter(value)).expect("counter"), value);
        }
        assert!(parse_counter(&[0; 7]).is_err());
        assert!(parse_counter(&[0; 8]).is_err());
        let time = DateTime::parse_from_rfc3339("2026-08-03T12:34:56.123456789Z")
            .expect("time")
            .with_timezone(&Utc);
        assert_eq!(parse_timestamp(&timestamp(&time)).expect("time"), time);
        assert!(parse_timestamp("2026-08-03T12:34:56+00:00").is_err());
        assert!(parse_fingerprint(&[0; 31]).is_err());
        assert!(parse_operation(9).is_err());
        assert!(parse_change_kind(-1).is_err());
    }

    #[test]
    fn source_order_rejects_partial_and_empty_columns() {
        assert!(parse_source_order(0, Some("sequence".to_string()), None).is_err());
        assert!(parse_source_order(1, None, None).is_err());
        assert!(parse_source_order(1, Some("sequence".to_string()), Some(Vec::new())).is_err());
    }
}
