use serde_json::Value;
use stacker::services::runtime_env_contract_response;

/// Load the shared runtime env contract.
///
/// Note: This contract is mirrored from
/// ../config/shared-fixtures/runtime-env-contract.json
/// to stacker/tests/contracts/runtime-env-contract.contract.json
/// for reliable CI access.
fn load_contract() -> Value {
    let contract_json = include_str!("contracts/runtime-env-contract.contract.json");
    serde_json::from_str(contract_json).expect("contract JSON should be valid")
}

#[test]
fn runtime_env_contract_has_expected_metadata() {
    let contract = load_contract();

    assert_eq!(contract["title"].as_str(), Some("runtime-env-contract"));
    assert_eq!(contract["_owner"].as_str(), Some("trydirect/config"));
    assert_eq!(contract["version"].as_str(), Some("v1"));
    assert_eq!(contract["order"].as_str(), Some("lowest_to_highest"));
}

#[test]
fn stacker_runtime_env_contract_matches_shared_fixture() {
    let contract = load_contract();
    let exported = serde_json::to_value(runtime_env_contract_response())
        .expect("runtime env contract should serialize");

    assert_eq!(exported["version"], contract["version"]);
    assert_eq!(exported["order"], contract["order"]);
    assert_eq!(exported["layers"], contract["layers"]);
}

#[test]
fn runtime_env_contract_inspection_fields_match_expected_outputs() {
    let contract = load_contract();

    assert_eq!(
        contract["inspectionOutputs"]["runtimeEnvContractField"].as_str(),
        Some("runtime_env_contract")
    );
    assert_eq!(
        contract["inspectionOutputs"]["remoteSecretMetadata"]["source"].as_str(),
        Some("vault")
    );
    assert_eq!(
        contract["inspectionOutputs"]["remoteSecretMetadata"]["secure"].as_bool(),
        Some(true)
    );
}
