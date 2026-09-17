use serde_json::Value;
use stacker::cli::config_parser::FieldPolicy;

/// Load the shared marketplace field-policy contract.
///
/// Note: This contract is mirrored from
/// ../config/shared-fixtures/api-contracts/marketplace-field-policy.json
/// to stacker/tests/contracts/marketplace-field-policy.contract.json
/// for reliable CI access.
fn load_contract() -> Value {
    let contract_json = include_str!("contracts/marketplace-field-policy.contract.json");
    serde_json::from_str(contract_json).expect("contract JSON should be valid")
}

#[test]
fn field_policy_contract_has_expected_metadata() {
    let contract = load_contract();

    assert_eq!(contract["title"].as_str(), Some("marketplace-field-policy"));
    assert_eq!(contract["_owner"].as_str(), Some("stacker"));
    assert_eq!(contract["version"].as_str(), Some("v1"));
}

#[test]
fn field_policy_accepts_every_contract_example() {
    let contract = load_contract();
    let examples = contract["examples"]
        .as_array()
        .expect("contract should have an examples array");
    assert!(
        !examples.is_empty(),
        "contract must ship at least one example"
    );

    for example in examples {
        let fields = example
            .as_object()
            .expect("each example should be a field-name -> policy object");
        for (field_name, policy_json) in fields {
            let parsed: Result<FieldPolicy, _> = serde_json::from_value(policy_json.clone());
            assert!(
                parsed.is_ok(),
                "field {field_name} should parse as a valid FieldPolicy: {:?}",
                parsed.err()
            );
        }
    }
}

#[test]
fn field_policy_rejects_every_contract_invalid_example() {
    let contract = load_contract();
    let invalid_examples = contract["invalidExamples"]
        .as_array()
        .expect("contract should have an invalidExamples array");
    assert!(
        !invalid_examples.is_empty(),
        "contract must ship at least one invalid example"
    );

    for invalid_example in invalid_examples {
        let reason = invalid_example["reason"]
            .as_str()
            .expect("invalid example should carry a reason");
        let fields = invalid_example["fields"]
            .as_object()
            .expect("invalid example should carry a fields object");

        for (field_name, policy_json) in fields {
            let parsed: Result<FieldPolicy, _> = serde_json::from_value(policy_json.clone());
            assert!(
                parsed.is_err(),
                "field {field_name} should be rejected ({reason}), but parsed as {:?}",
                parsed.ok()
            );
        }
    }
}
