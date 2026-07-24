mod fixture_helpers;

use attention_protocol::IngestSourceOccurrenceParams;
use attention_protocol::MutationResult;
use attention_protocol::SourceIngestionValue;
use fixture_helpers::assert_raw_invalid;
use fixture_helpers::assert_structural_round_trip;
use serde_json::Value;

#[test]
fn applied_and_replayed_results_preserve_commit_identity() {
    assert_structural_round_trip::<Vec<MutationResult<Value>>>("methods/mutations/results.json");
}

#[test]
fn every_source_ingress_decision_round_trips() {
    assert_structural_round_trip::<Vec<SourceIngestionValue>>(
        "methods/source_ingress/decisions.json",
    );
    assert_raw_invalid::<SourceIngestionValue>("methods/source_ingress/invalid_decision.json");
}

#[test]
fn source_ingress_params_omit_server_generated_ingested_at() {
    assert_structural_round_trip::<IngestSourceOccurrenceParams>(
        "methods/source_ingress/params.json",
    );
}
