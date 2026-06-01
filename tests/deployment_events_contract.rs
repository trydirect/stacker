use serde_json::Value;

use stacker::services::{DeploymentEventFeed, DEPLOYMENT_EVENTS_SCHEMA_VERSION};

fn load_contract() -> Value {
    serde_json::from_str(include_str!(
        "contracts/stacker-deployment-events.v1alpha1.contract.json"
    ))
    .expect("deployment events contract JSON should be valid")
}

#[test]
fn deployment_events_contract_metadata_is_correct() {
    let contract = load_contract();

    assert_eq!(
        contract["title"].as_str().unwrap(),
        "stacker-deployment-events-v1alpha1"
    );
    assert_eq!(contract["_owner"].as_str().unwrap(), "stacker");
}

#[test]
fn deployment_events_contract_requires_core_fields() {
    let contract = load_contract();
    let required = contract["response"]["required"]
        .as_array()
        .expect("required should be an array");
    let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();

    for field in ["schemaVersion", "deploymentHash", "events"] {
        assert!(
            required_names.contains(&field),
            "required fields must include {field}"
        );
    }
}

#[test]
fn deployment_events_fixture_deserializes_into_shared_type() {
    let feed: DeploymentEventFeed = serde_json::from_str(include_str!(
        "contracts/stacker-deployment-events.v1alpha1.sample.json"
    ))
    .expect("deployment events fixture should deserialize");

    assert_eq!(feed.schema_version, DEPLOYMENT_EVENTS_SCHEMA_VERSION);
    assert_eq!(feed.events.len(), 3);
    assert_eq!(feed.events[0].sequence, 1);
}
