use serde_json::Value;

use stacker::services::deployment_state::{DeploymentState, DEPLOYMENT_STATE_SCHEMA_VERSION};

fn load_contract() -> Value {
    serde_json::from_str(include_str!(
        "contracts/stacker-deployment-state.v1alpha1.contract.json"
    ))
    .expect("deployment state contract JSON should be valid")
}

fn load_online_fixture() -> &'static str {
    include_str!("contracts/stacker-deployment-state.v1alpha1.online.json")
}

fn load_offline_fixture() -> &'static str {
    include_str!("contracts/stacker-deployment-state.v1alpha1.offline.json")
}

#[test]
fn deployment_state_contract_metadata_is_correct() {
    let contract = load_contract();

    assert_eq!(
        contract["title"].as_str().unwrap(),
        "stacker-deployment-state-v1alpha1"
    );
    assert_eq!(contract["_owner"].as_str().unwrap(), "stacker");
    assert_eq!(
        contract["endpoint"]["path"].as_str().unwrap(),
        "/api/v1/deployments/{deployment_hash}/state"
    );
}

#[test]
fn deployment_state_contract_requires_canonical_top_level_fields() {
    let contract = load_contract();
    let required = contract["response"]["required"]
        .as_array()
        .expect("required should be an array");
    let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();

    for field in [
        "schemaVersion",
        "project",
        "deployment",
        "agent",
        "runtime",
        "apps",
        "drift",
    ] {
        assert!(
            required_names.contains(&field),
            "required fields must include {field}"
        );
    }
}

#[test]
fn online_fixture_deserializes_into_shared_deployment_state_type() {
    let state: DeploymentState =
        serde_json::from_str(load_online_fixture()).expect("online fixture should deserialize");

    assert_eq!(state.schema_version, DEPLOYMENT_STATE_SCHEMA_VERSION);
    assert_eq!(state.deployment.deployment_hash, "deployment_state_online");
    assert_eq!(state.agent.status, "online");
    assert_eq!(state.apps.len(), 2);
}

#[test]
fn offline_fixture_deserializes_and_omits_optional_agent_fields() {
    let state: DeploymentState =
        serde_json::from_str(load_offline_fixture()).expect("offline fixture should deserialize");

    assert_eq!(state.schema_version, DEPLOYMENT_STATE_SCHEMA_VERSION);
    assert_eq!(state.deployment.deployment_hash, "deployment_state_offline");
    assert_eq!(state.agent.status, "offline");
    assert!(state.agent.id.is_none());
    assert!(state.last_command.is_none());
}
