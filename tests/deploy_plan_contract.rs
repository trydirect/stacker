use serde_json::Value;

use stacker::services::{DeployPlan, DEPLOY_PLAN_SCHEMA_VERSION};

fn load_contract() -> Value {
    serde_json::from_str(include_str!(
        "contracts/stacker-deploy-plan.v1alpha1.contract.json"
    ))
    .expect("deploy plan contract JSON should be valid")
}

#[test]
fn deploy_plan_contract_metadata_is_correct() {
    let contract = load_contract();

    assert_eq!(
        contract["title"].as_str().unwrap(),
        "stacker-deploy-plan-v1alpha1"
    );
    assert_eq!(contract["_owner"].as_str().unwrap(), "stacker");
}

#[test]
fn deploy_plan_contract_requires_core_fields() {
    let contract = load_contract();
    let required = contract["response"]["required"]
        .as_array()
        .expect("required should be an array");
    let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();

    for field in [
        "schemaVersion",
        "deploymentHash",
        "operation",
        "target",
        "fingerprint",
        "scope",
        "hasChanges",
        "actions",
        "reasoning",
    ] {
        assert!(
            required_names.contains(&field),
            "required fields must include {field}"
        );
    }
}

#[test]
fn no_changes_fixture_deserializes_into_shared_type() {
    let plan: DeployPlan = serde_json::from_str(include_str!(
        "contracts/stacker-deploy-plan.v1alpha1.no-changes.json"
    ))
    .expect("no changes fixture should deserialize");

    assert_eq!(plan.schema_version, DEPLOY_PLAN_SCHEMA_VERSION);
    assert!(!plan.has_changes);
    assert!(plan.actions.is_empty());
}

#[test]
fn env_drift_fixture_deserializes_into_shared_type() {
    let plan: DeployPlan = serde_json::from_str(include_str!(
        "contracts/stacker-deploy-plan.v1alpha1.env-drift.json"
    ))
    .expect("env drift fixture should deserialize");

    assert_eq!(plan.schema_version, DEPLOY_PLAN_SCHEMA_VERSION);
    assert!(plan.has_changes);
    assert_eq!(plan.actions.len(), 2);
}

#[test]
fn deploy_app_fixture_deserializes_into_shared_type() {
    let plan: DeployPlan = serde_json::from_str(include_str!(
        "contracts/stacker-deploy-plan.v1alpha1.deploy-app.json"
    ))
    .expect("deploy-app fixture should deserialize");

    assert_eq!(plan.schema_version, DEPLOY_PLAN_SCHEMA_VERSION);
    assert!(plan.has_changes);
    assert_eq!(plan.scope.mode, "app");
    assert_eq!(plan.scope.app_code.as_deref(), Some("upload"));
}

#[test]
fn rollback_fixture_deserializes_into_shared_type() {
    let plan: DeployPlan = serde_json::from_str(include_str!(
        "contracts/stacker-deploy-plan.v1alpha1.rollback-previous.json"
    ))
    .expect("rollback fixture should deserialize");

    assert_eq!(plan.schema_version, DEPLOY_PLAN_SCHEMA_VERSION);
    assert!(plan.has_changes);
    assert_eq!(
        plan.operation,
        stacker::services::DeployPlanOperation::RollbackDeploy
    );
    assert_eq!(
        plan.rollback
            .as_ref()
            .map(|item| item.resolved_version.as_str()),
        Some("1.1.0")
    );
}
