use serde_json::Value;

use stacker::services::{ExplainEnv, ExplainTopology, EXPLAIN_SCHEMA_VERSION};

fn load_contract(path: &str) -> Value {
    serde_json::from_str(path).expect("contract JSON should be valid")
}

#[test]
fn explain_env_contract_metadata_is_correct() {
    let contract = load_contract(include_str!(
        "contracts/stacker-explain-env.v1alpha1.contract.json"
    ));

    assert_eq!(
        contract["title"].as_str().unwrap(),
        "stacker-explain-env-v1alpha1"
    );
    assert_eq!(contract["_owner"].as_str().unwrap(), "stacker");
}

#[test]
fn explain_topology_contract_metadata_is_correct() {
    let contract = load_contract(include_str!(
        "contracts/stacker-explain-topology.v1alpha1.contract.json"
    ));

    assert_eq!(
        contract["title"].as_str().unwrap(),
        "stacker-explain-topology-v1alpha1"
    );
    assert_eq!(contract["_owner"].as_str().unwrap(), "stacker");
}

#[test]
fn explain_env_fixture_deserializes_and_stays_redacted() {
    let fixture = include_str!("contracts/stacker-explain-env.v1alpha1.json");
    let explain: ExplainEnv =
        serde_json::from_str(fixture).expect("env fixture should deserialize");

    assert_eq!(explain.schema_version, EXPLAIN_SCHEMA_VERSION);
    assert_eq!(explain.deployment_hash, "deployment_state_online");
    assert_eq!(explain.app_code, "device-api");
    assert!(!fixture.contains("SUPER_SECRET_SHOULD_NOT_LEAK"));
}

#[test]
fn explain_topology_fixture_deserializes() {
    let fixture = include_str!("contracts/stacker-explain-topology.v1alpha1.json");
    let explain: ExplainTopology =
        serde_json::from_str(fixture).expect("topology fixture should deserialize");

    assert_eq!(explain.schema_version, EXPLAIN_SCHEMA_VERSION);
    assert_eq!(explain.target, "cloud");
    assert_eq!(explain.services.len(), 2);
}
