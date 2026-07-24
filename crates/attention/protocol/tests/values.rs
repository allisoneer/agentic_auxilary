mod fixture_helpers;

use attention_protocol::AttentionSignalId;
use attention_protocol::ChangeEventId;
use attention_protocol::DeliveryLeaseToken;
use attention_protocol::MutationIdempotencyKey;
use attention_protocol::NormalizedSourceOrder;
use attention_protocol::OutboxIntentId;
use attention_protocol::ProviderMessageId;
use attention_protocol::ReminderFireId;
use attention_protocol::ReminderId;
use attention_protocol::Revision;
use attention_protocol::SourceEntityId;
use attention_protocol::SourceReceiptId;
use attention_protocol::SourceStateVersion;
use attention_protocol::WorkItemId;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize)]
struct ValuesFixture {
    work_item_id: WorkItemId,
    attention_signal_id: AttentionSignalId,
    reminder_id: ReminderId,
    reminder_fire_id: ReminderFireId,
    source_receipt_id: SourceReceiptId,
    source_entity_id: SourceEntityId,
    change_event_id: ChangeEventId,
    outbox_intent_id: OutboxIntentId,
    mutation_idempotency_key: MutationIdempotencyKey,
    provider_message_id: ProviderMessageId,
    revision: Revision,
    source_state_version: SourceStateVersion,
    normalized_source_order: NormalizedSourceOrder,
    delivery_lease_token: DeliveryLeaseToken,
}

#[test]
fn all_v1_ids_and_values_round_trip() {
    assert_structural_round_trip::<ValuesFixture>("ids/v1_values.json");
}

#[test]
fn decimal_and_base64_invalid_fixtures_are_rejected() {
    for fixture in [
        "ids/invalid_revision_zero.json",
        "ids/invalid_revision_leading_zero.json",
    ] {
        assert_raw_invalid::<Revision>(fixture);
    }
    for fixture in [
        "ids/invalid_base64_padding.json",
        "ids/invalid_base64_trailing_bits.json",
    ] {
        assert_raw_invalid::<NormalizedSourceOrder>(fixture);
    }
    assert_raw_invalid::<DeliveryLeaseToken>("ids/invalid_lease_length.json");
}

#[test]
fn durable_ids_have_distinct_api_types() {
    fn work_item(_: WorkItemId) {}
    fn signal(_: AttentionSignalId) {}
    work_item(WorkItemId("work-item-1".to_string()));
    signal(AttentionSignalId("signal-1".to_string()));
    assert_ne!(
        std::any::TypeId::of::<WorkItemId>(),
        std::any::TypeId::of::<AttentionSignalId>()
    );
}
