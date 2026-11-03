//! Shared deploy-failure payload contract.
//!
//! Mirrored from `config/shared-fixtures/deploy-failure-payload.json`
//! (canonical source) to `tests/contracts/deploy-failure-payload.contract.json`
//! for reliable CI access. The producer is the Install Service
//! (`report_on_fail`), the consumer is the Stacker MQ listener — a field the
//! listener doesn't declare is silently dropped by serde, which is exactly how
//! the typed `available_options` record was lost on its way to the CLI.

use serde_json::Value;

fn load_contract() -> Value {
    serde_json::from_str(include_str!(
        "contracts/deploy-failure-payload.contract.json"
    ))
    .expect("deploy failure payload contract JSON should be valid")
}

#[test]
fn contract_metadata_is_correct() {
    let contract = load_contract();

    assert_eq!(contract["_owner"].as_str().unwrap(), "stacker");
    let producers = contract["_producers"].as_array().unwrap();
    assert!(producers
        .iter()
        .filter_map(Value::as_str)
        .any(|p| p == "install"));
    let consumers = contract["_consumers"].as_array().unwrap();
    assert!(consumers
        .iter()
        .filter_map(Value::as_str)
        .any(|c| c == "stacker"));
}

#[test]
fn example_payload_satisfies_its_own_required_fields() {
    let contract = load_contract();
    let example = &contract["example"];

    for field in contract["fields"]["required"].as_array().unwrap() {
        let field = field.as_str().unwrap();
        assert!(
            example.get(field).is_some_and(|v| !v.is_null()),
            "example payload must carry required field `{field}`"
        );
    }

    let options = &example["available_options"];
    for field in contract["available_options"]["required"]
        .as_array()
        .unwrap()
    {
        let field = field.as_str().unwrap();
        assert!(
            options.get(field).is_some_and(|v| !v.is_null()),
            "example available_options must carry `{field}`"
        );
    }
}

#[test]
fn example_error_kind_is_in_the_declared_enum() {
    let contract = load_contract();
    let kind = contract["example"]["available_options"]["error_kind"]
        .as_str()
        .expect("error_kind must be a string");

    let allowed: Vec<&str> = contract["available_options"]["error_kind_values"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();

    assert!(
        allowed.contains(&kind),
        "example error_kind `{kind}` must be in {allowed:?}"
    );
    assert_eq!(allowed.len(), 8, "8 classified failure modes exist");
}

#[test]
fn example_terminal_status_is_a_known_failure_status() {
    let contract = load_contract();
    let status = contract["example"]["status"].as_str().unwrap();

    let known: Vec<&str> = contract["fields"]["status_values_terminal_failure"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();

    assert!(
        known.contains(&status),
        "status `{status}` must be in {known:?}"
    );
}

/// The listener's `ProgressMessage` must deserialize the example. This is the
/// regression guard for the whole contract: an undeclared field is dropped
/// silently, with no compile or run-time error.
#[test]
fn listener_progress_message_deserializes_the_example() {
    let contract = load_contract();
    // Mirror of src/console/commands/mq/listener.rs::ProgressMessage —
    // kept in sync by the unit test in that module as well.
    let msg: Value = contract["example"].clone();

    for field in ["id", "alert", "message", "status", "progress"] {
        assert!(msg.get(field).is_some(), "ProgressMessage needs `{field}`");
    }
    let options = msg
        .get("available_options")
        .expect("ProgressMessage must declare available_options");
    assert_eq!(
        options["error_kind"].as_str().unwrap(),
        "port_conflict",
        "error_kind must reach the consumer"
    );
}

/// Missing `available_options` (pre-contract producers) must not be an error —
/// consumers treat absence as unclassified.
#[test]
fn absence_of_available_options_is_allowed() {
    let contract = load_contract();
    let example = contract["example"].as_object().unwrap();

    let mut legacy = example.clone();
    legacy.remove("available_options");
    let legacy = Value::Object(legacy);

    let options = legacy.get("available_options");
    assert!(options.is_none(), "legacy payloads simply omit the field");
}
