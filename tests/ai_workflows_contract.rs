use serde_json::Value;

fn load_workflows() -> Value {
    serde_json::from_str(include_str!("contracts/stacker-ai-workflows.v1alpha1.json"))
        .expect("AI workflow fixture should be valid JSON")
}

#[test]
fn ai_workflows_fixture_metadata_is_correct() {
    let workflows = load_workflows();

    assert_eq!(
        workflows["title"].as_str().unwrap(),
        "stacker-ai-workflows-v1alpha1"
    );
    assert_eq!(workflows["_owner"].as_str().unwrap(), "stacker");
    assert_eq!(workflows["version"].as_str().unwrap(), "v1alpha1");
}

#[test]
fn ai_workflows_cover_inspect_explain_plan_apply_and_recover() {
    let workflows = load_workflows();
    let workflow_items = workflows["workflows"]
        .as_array()
        .expect("workflows should be an array");

    let inspect_apply = workflow_items
        .iter()
        .find(|workflow| workflow["name"] == "inspect-explain-plan-apply")
        .expect("inspect/apply workflow should exist");
    let inspect_apply_steps = inspect_apply["steps"]
        .as_array()
        .expect("inspect/apply workflow steps should be an array");
    let inspect_apply_tools: Vec<&str> = inspect_apply_steps
        .iter()
        .filter_map(|step| step["tool"].as_str())
        .collect();
    assert_eq!(
        inspect_apply_tools,
        vec![
            "get_deployment_state",
            "explain_topology",
            "get_deployment_plan",
            "apply_deployment_plan",
        ]
    );

    let recover = workflow_items
        .iter()
        .find(|workflow| workflow["name"] == "recover-with-rollback")
        .expect("rollback recovery workflow should exist");
    let recover_steps = recover["steps"]
        .as_array()
        .expect("rollback recovery steps should be an array");
    let recover_tools: Vec<&str> = recover_steps
        .iter()
        .filter_map(|step| step["tool"].as_str())
        .collect();
    assert_eq!(
        recover_tools,
        vec![
            "get_deployment_state",
            "get_deployment_events",
            "get_deployment_plan",
            "apply_deployment_plan",
            "get_deployment_events",
        ]
    );
}

#[test]
fn ai_workflows_require_confirmation_and_fingerprint_for_apply_steps() {
    let workflows = load_workflows();
    let workflow_items = workflows["workflows"]
        .as_array()
        .expect("workflows should be an array");

    let apply_steps: Vec<&Value> = workflow_items
        .iter()
        .flat_map(|workflow| {
            workflow["steps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|step| step["tool"] == "apply_deployment_plan")
        })
        .collect();

    assert_eq!(apply_steps.len(), 2, "expected two apply workflow steps");
    for step in apply_steps {
        assert_eq!(step["confirmRequired"].as_bool(), Some(true));
        assert_eq!(step["requiresFingerprint"].as_bool(), Some(true));
        assert_eq!(step["requiresMfa"].as_bool(), Some(true));
    }
}
