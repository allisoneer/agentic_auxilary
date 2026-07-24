mod fixture_helpers;

use attention_protocol::DeliveryAuthorityView;
use attention_protocol::DeliveryClaim;
use attention_protocol::DeliveryClaimParams;
use attention_protocol::DeliveryClaimResult;
use attention_protocol::DeliveryCompletionResult;
use attention_protocol::DeliveryFailRetryable;
use attention_protocol::DeliveryFailRetryableParams;
use attention_protocol::DeliveryFailTerminal;
use attention_protocol::DeliveryFailTerminalParams;
use attention_protocol::DeliveryInspect;
use attention_protocol::DeliveryInspectParams;
use attention_protocol::DeliveryRenew;
use attention_protocol::DeliveryRenewParams;
use attention_protocol::DeliveryRenewResult;
use attention_protocol::DeliveryStateView;
use attention_protocol::DeliverySucceed;
use attention_protocol::DeliverySucceedParams;
use attention_protocol::RpcMethod;
use attention_protocol::RpcRequest;
use attention_protocol::V1_METHOD_NAMES;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use fixture_helpers::fixture_text;
use serde_json::Value;

#[expect(clippy::expect_used, reason = "fixture assertion helper")]
fn assert_request<M>(value: &Value)
where
    M: RpcMethod,
    M::Params: Clone,
{
    let request: RpcRequest<M::Params> =
        serde_json::from_value(value.clone()).expect("typed worker request");
    let constructed = M::request(
        request.id.clone(),
        request.params.clone().expect("worker params"),
    );
    assert_eq!(request.method, M::NAME);
    assert_eq!(
        serde_json::to_value(constructed).expect("serialize"),
        *value
    );
}

fn assert_binding<M, P, R>()
where
    M: RpcMethod<Params = P, Result = R>,
    P: serde::Serialize + serde::de::DeserializeOwned,
    R: serde::Serialize + serde::de::DeserializeOwned,
{
}

#[test]
fn all_six_worker_requests_use_typed_catalog_bindings() {
    let requests: Vec<Value> =
        serde_json::from_str(&fixture_text("workers/requests.json")).expect("worker requests");
    assert_request::<DeliveryClaim>(&requests[0]);
    assert_request::<DeliveryInspect>(&requests[1]);
    assert_request::<DeliveryRenew>(&requests[2]);
    assert_request::<DeliverySucceed>(&requests[3]);
    assert_request::<DeliveryFailRetryable>(&requests[4]);
    assert_request::<DeliveryFailTerminal>(&requests[5]);

    assert_binding::<DeliveryClaim, DeliveryClaimParams, DeliveryClaimResult>();
    assert_binding::<DeliveryInspect, DeliveryInspectParams, DeliveryAuthorityView>();
    assert_binding::<DeliveryRenew, DeliveryRenewParams, DeliveryRenewResult>();
    assert_binding::<DeliverySucceed, DeliverySucceedParams, DeliveryCompletionResult>();
    assert_binding::<DeliveryFailRetryable, DeliveryFailRetryableParams, DeliveryCompletionResult>(
    );
    assert_binding::<DeliveryFailTerminal, DeliveryFailTerminalParams, DeliveryCompletionResult>();
}

#[test]
fn all_delivery_states_and_worker_outcomes_round_trip() {
    assert_structural_round_trip::<Vec<DeliveryStateView>>("workers/states.json");
    assert_structural_round_trip::<DeliveryClaimResult>("workers/claim_result.json");
    assert_structural_round_trip::<Vec<DeliveryRenewResult>>("workers/renewal_outcomes.json");
    assert_structural_round_trip::<Vec<DeliveryCompletionResult>>(
        "workers/completion_outcomes.json",
    );
    assert_raw_invalid::<DeliveryStateView>("workers/invalid_unknown_status.json");
}

#[test]
fn inspection_never_serializes_active_lease_tokens() {
    assert_structural_round_trip::<DeliveryAuthorityView>(
        "workers/authority_reminder_skipped.json",
    );
    let authority: DeliveryAuthorityView =
        serde_json::from_str(&fixture_text("workers/authority_leased.json"))
            .expect("leased authority");
    let value = serde_json::to_value(authority).expect("serialize authority");
    assert!(value["state"].get("lease_token").is_none());

    let claim: Value =
        serde_json::from_str(&fixture_text("workers/claim_result.json")).expect("claim result");
    assert!(claim["claims"][0].get("lease_token").is_some());
}

#[test]
fn no_public_skip_or_checkpoint_method_exists() {
    for prohibited in [
        "attention.delivery.skip",
        "attention.delivery.checkpoint.get",
        "attention.delivery.checkpoint.advance",
    ] {
        assert!(!V1_METHOD_NAMES.contains(&prohibited));
    }
}
