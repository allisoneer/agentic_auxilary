use attention_kernel::AttentionSignalId;
use attention_kernel::ChangeEventId;
use attention_kernel::ExternalEntityId;
use attention_kernel::InvariantError;
use attention_kernel::OccurrenceId;
use attention_kernel::OccurrenceKey;
use attention_kernel::OutboxIntentId;
use attention_kernel::ReminderFireId;
use attention_kernel::ReminderId;
use attention_kernel::SourceEntityId;
use attention_kernel::SourceEntityKey;
use attention_kernel::SourceInstance;
use attention_kernel::SourceKind;
use attention_kernel::SourceReceiptId;
use attention_kernel::WorkItemId;
use std::str::FromStr;
use uuid::Uuid;
use uuid::Version;

macro_rules! assert_native_id {
    ($type:ty) => {{
        let id = <$type>::new();
        assert_eq!(id.as_uuid().get_version(), Some(Version::SortRand));
        let text = id.to_string();
        assert_eq!(<$type>::from_str(&text).expect("canonical ID"), id);
        assert!(matches!(
            <$type>::from_str(&text.to_uppercase()),
            Err(InvariantError::NonCanonicalUuidText(_))
        ));
        assert!(matches!(
            <$type>::try_from(Uuid::nil()),
            Err(InvariantError::InvalidUuidVersion)
        ));
    }};
}

#[test]
fn all_eight_native_ids_are_distinct_uuid_v7_types() {
    assert_native_id!(WorkItemId);
    assert_native_id!(AttentionSignalId);
    assert_native_id!(ReminderId);
    assert_native_id!(ReminderFireId);
    assert_native_id!(SourceReceiptId);
    assert_native_id!(SourceEntityId);
    assert_native_id!(ChangeEventId);
    assert_native_id!(OutboxIntentId);
}

#[test]
fn malformed_and_non_v7_text_is_rejected() {
    assert!(matches!(
        WorkItemId::from_str("not-a-uuid"),
        Err(InvariantError::InvalidUuidText(_))
    ));
    assert!(matches!(
        WorkItemId::from_str("00000000-0000-0000-0000-000000000000"),
        Err(InvariantError::InvalidUuidVersion)
    ));
    let id = WorkItemId::new().to_string().replace('-', "");
    assert!(matches!(
        WorkItemId::from_str(&id),
        Err(InvariantError::NonCanonicalUuidText(_))
    ));
}

#[test]
fn source_components_and_composite_keys_are_validated_and_comparable() {
    assert!(SourceKind::new("").is_err());
    assert!(SourceInstance::new("  ").is_err());
    assert!(OccurrenceId::new("").is_err());
    assert!(ExternalEntityId::new("").is_err());

    let kind = SourceKind::new("linear").expect("source kind");
    let instance = SourceInstance::new("workspace").expect("source instance");
    let occurrence = OccurrenceKey::new(
        kind.clone(),
        instance.clone(),
        OccurrenceId::new("event-1").expect("occurrence ID"),
    );
    let same_occurrence = occurrence.clone();
    assert_eq!(occurrence, same_occurrence);

    let entity = SourceEntityKey::new(
        kind,
        instance,
        ExternalEntityId::new("ENG-1117").expect("external entity ID"),
    );
    assert_eq!(entity.external_entity_id().as_str(), "ENG-1117");
}
