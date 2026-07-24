use serde_json::Value;

use stacker::services::{
    TypedErrorCode, TypedErrorEnvelope, TypedRemediationClass, TYPED_ERROR_SCHEMA_VERSION,
};

fn load_contract() -> Value {
    serde_json::from_str(include_str!(
        "contracts/stacker-typed-error.v1alpha1.contract.json"
    ))
    .expect("typed error contract JSON should be valid")
}

fn load_fixture() -> &'static str {
    include_str!("contracts/stacker-typed-error.v1alpha1.deployment-capability-missing.json")
}

#[test]
fn typed_error_contract_metadata_is_correct() {
    let contract = load_contract();

    assert_eq!(
        contract["title"].as_str().unwrap(),
        "stacker-typed-error-v1alpha1"
    );
    assert_eq!(contract["_owner"].as_str().unwrap(), "stacker");
    assert!(contract["surfaces"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .any(|surface| surface == "mcp"));
}

#[test]
fn typed_error_contract_requires_core_fields() {
    let contract = load_contract();
    let required = contract["response"]["required"]
        .as_array()
        .expect("required should be an array");
    let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();

    for field in [
        "schemaVersion",
        "code",
        "message",
        "retryable",
        "remediationClass",
    ] {
        assert!(
            required_names.contains(&field),
            "required fields must include {field}"
        );
    }
}

#[test]
fn typed_error_fixture_deserializes_into_shared_type() {
    let error: TypedErrorEnvelope =
        serde_json::from_str(load_fixture()).expect("typed error fixture should deserialize");

    assert_eq!(error.schema_version, TYPED_ERROR_SCHEMA_VERSION);
    assert_eq!(error.code, TypedErrorCode::DeploymentCapabilityMissing);
    assert_eq!(error.remediation_class, TypedRemediationClass::Capability);
    assert!(!error.retryable);
    assert_eq!(
        error.context.get("capability").map(String::as_str),
        Some("compose_logs")
    );
}
