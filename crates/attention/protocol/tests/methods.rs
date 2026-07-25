mod fixture_helpers;

use attention_protocol::AcknowledgeAttentionSignalParams;
use attention_protocol::AcknowledgeAttentionSignalResult;
use attention_protocol::AcknowledgeReminderFireParams;
use attention_protocol::AcknowledgeReminderFireResult;
use attention_protocol::AttentionChange;
use attention_protocol::AttentionSignalAcknowledge;
use attention_protocol::AttentionSignalGet;
use attention_protocol::AttentionSignalGetParams;
use attention_protocol::AttentionSignalView;
use attention_protocol::CancelWorkItemParams;
use attention_protocol::CancelWorkItemResult;
use attention_protocol::ChangeNotificationParams;
use attention_protocol::ChangesGet;
use attention_protocol::ChangesGetParams;
use attention_protocol::ChangesResult;
use attention_protocol::CompleteWorkItemParams;
use attention_protocol::CompleteWorkItemResult;
use attention_protocol::CreateReminderParams;
use attention_protocol::CreateReminderResult;
use attention_protocol::CreateWorkItemParams;
use attention_protocol::CreateWorkItemResult;
use attention_protocol::EmptyParams;
use attention_protocol::IngestSourceOccurrenceParams;
use attention_protocol::IngestSourceOccurrenceResult;
use attention_protocol::ReminderCreate;
use attention_protocol::ReminderFireAcknowledge;
use attention_protocol::ReminderFireSnooze;
use attention_protocol::ReminderGet;
use attention_protocol::ReminderGetParams;
use attention_protocol::ReminderView;
use attention_protocol::RpcMethod;
use attention_protocol::RpcNotification;
use attention_protocol::RpcNotificationMethod;
use attention_protocol::RpcRequest;
use attention_protocol::SnapshotGet;
use attention_protocol::SnapshotResult;
use attention_protocol::SnoozeReminderFireParams;
use attention_protocol::SnoozeReminderFireResult;
use attention_protocol::SourceEntityGet;
use attention_protocol::SourceEntityGetParams;
use attention_protocol::SourceEntityView;
use attention_protocol::SourceOccurrenceIngest;
use attention_protocol::SourceReceiptGet;
use attention_protocol::SourceReceiptGetParams;
use attention_protocol::SourceReceiptView;
use attention_protocol::V1_METHOD_NAMES;
use attention_protocol::WorkItemCancel;
use attention_protocol::WorkItemComplete;
use attention_protocol::WorkItemCreate;
use attention_protocol::WorkItemGet;
use attention_protocol::WorkItemGetParams;
use attention_protocol::WorkItemView;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use fixture_helpers::fixture_text;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[expect(clippy::expect_used, reason = "fixture assertion helper")]
fn assert_request<M>(value: &Value)
where
    M: RpcMethod,
    M::Params: Clone,
{
    let request: RpcRequest<M::Params> =
        serde_json::from_value(value.clone()).expect("typed request fixture");
    assert_eq!(request.method, M::NAME);
    let constructed = M::request(
        request.id.clone(),
        request.params.clone().expect("known method params"),
    );
    assert_eq!(
        serde_json::to_value(constructed).expect("constructed request serialization"),
        *value
    );
    assert_eq!(
        serde_json::to_value(request).expect("request serialization"),
        *value
    );
}

fn assert_binding<M, P, R>()
where
    M: RpcMethod<Params = P, Result = R>,
    P: Serialize + DeserializeOwned,
    R: Serialize + DeserializeOwned,
{
}

#[test]
fn every_core_request_fixture_uses_its_concrete_catalog_binding() {
    let queries: Vec<Value> = serde_json::from_str(&fixture_text("methods/queries/requests.json"))
        .expect("query request fixtures");
    assert_request::<WorkItemGet>(&queries[0]);
    assert_request::<AttentionSignalGet>(&queries[1]);
    assert_request::<ReminderGet>(&queries[2]);
    assert_request::<SourceEntityGet>(&queries[3]);
    assert_request::<SourceReceiptGet>(&queries[4]);
    assert_request::<SnapshotGet>(&queries[5]);
    assert_request::<ChangesGet>(&queries[6]);

    let mutations: Vec<Value> =
        serde_json::from_str(&fixture_text("methods/mutations/requests.json"))
            .expect("mutation request fixtures");
    assert_request::<WorkItemCreate>(&mutations[0]);
    assert_request::<WorkItemComplete>(&mutations[1]);
    assert_request::<WorkItemCancel>(&mutations[2]);
    assert_request::<AttentionSignalAcknowledge>(&mutations[3]);
    assert_request::<ReminderCreate>(&mutations[4]);
    assert_request::<ReminderFireAcknowledge>(&mutations[5]);
    assert_request::<ReminderFireSnooze>(&mutations[6]);

    let ingress: Value = serde_json::from_str(&fixture_text("methods/source_ingress/request.json"))
        .expect("source ingress request fixture");
    assert_request::<SourceOccurrenceIngest>(&ingress);
}

#[test]
fn every_core_marker_has_one_exact_params_and_result_binding() {
    assert_binding::<WorkItemGet, WorkItemGetParams, WorkItemView>();
    assert_binding::<AttentionSignalGet, AttentionSignalGetParams, AttentionSignalView>();
    assert_binding::<ReminderGet, ReminderGetParams, ReminderView>();
    assert_binding::<SourceEntityGet, SourceEntityGetParams, SourceEntityView>();
    assert_binding::<SourceReceiptGet, SourceReceiptGetParams, SourceReceiptView>();
    assert_binding::<SnapshotGet, EmptyParams, SnapshotResult>();
    assert_binding::<ChangesGet, ChangesGetParams, ChangesResult>();
    assert_binding::<WorkItemCreate, CreateWorkItemParams, CreateWorkItemResult>();
    assert_binding::<WorkItemComplete, CompleteWorkItemParams, CompleteWorkItemResult>();
    assert_binding::<WorkItemCancel, CancelWorkItemParams, CancelWorkItemResult>();
    assert_binding::<
        AttentionSignalAcknowledge,
        AcknowledgeAttentionSignalParams,
        AcknowledgeAttentionSignalResult,
    >();
    assert_binding::<
        SourceOccurrenceIngest,
        IngestSourceOccurrenceParams,
        IngestSourceOccurrenceResult,
    >();
    assert_binding::<ReminderCreate, CreateReminderParams, CreateReminderResult>();
    assert_binding::<
        ReminderFireAcknowledge,
        AcknowledgeReminderFireParams,
        AcknowledgeReminderFireResult,
    >();
    assert_binding::<ReminderFireSnooze, SnoozeReminderFireParams, SnoozeReminderFireResult>();
}

#[test]
fn change_notification_round_trips_through_its_marker() {
    assert_structural_round_trip::<RpcNotification<ChangeNotificationParams>>(
        "methods/change_notification.json",
    );
    let value: Value = serde_json::from_str(&fixture_text("methods/change_notification.json"))
        .expect("change notification fixture");
    let notification: RpcNotification<ChangeNotificationParams> =
        serde_json::from_value(value.clone()).expect("typed notification");
    assert_eq!(notification.method, AttentionChange::NAME);
    assert_eq!(
        serde_json::to_value(notification).expect("serialize notification"),
        value
    );
}

#[test]
fn unknown_methods_remain_open_and_prohibited_methods_are_absent() {
    assert_raw_invalid::<RpcRequest<EmptyParams>>("methods/invalid_array_params.json");
    let unknown: RpcRequest<Value> = serde_json::from_str(
        r#"{"jsonrpc":"2.0","id":"request-x","method":"attention.future","params":{}}"#,
    )
    .expect("open unknown request");
    assert_eq!(unknown.method, "attention.future");

    for prohibited in [
        "attention.reminder.fire",
        "attention.delivery.skip",
        "attention.delivery.checkpoint.get",
        "attention.delivery.checkpoint.advance",
    ] {
        assert!(!V1_METHOD_NAMES.contains(&prohibited));
    }
}
