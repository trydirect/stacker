use cucumber::{given, when};
use serde_json::json;

use super::StepWorld;

// ─── Background: create a pipe template for DAG tests ─────────

#[given(regex = r#"^I have a DAG pipe template named "([^"]+)"$"#)]
async fn given_dag_pipe_template(world: &mut StepWorld, name: String) {
    let pool = world.db_pool.as_ref().expect("no db_pool");

    // Use unique name per scenario to avoid parallel conflicts
    let unique_name = format!("{}-{}", name, uuid::Uuid::new_v4());

    let body = json!({
        "name": unique_name,
        "source_app_type": "test_source",
        "source_endpoint": {"path": "/api/in", "method": "GET"},
        "target_app_type": "test_target",
        "target_endpoint": {"path": "/api/out", "method": "POST"},
        "field_mapping": {"id": "id"},
        "is_public": false
    });

    let (_status, _body) = world.post_json("/api/v1/pipes/templates", &body).await;

    assert_eq!(
        world.status_code,
        Some(201),
        "Failed to create DAG pipe template '{}': {}",
        unique_name,
        world.response_body.as_deref().unwrap_or("<none>")
    );

    world.store_id_from_response("dag_template_id", "/item/id");
}

// ─── Step creation helpers ────────────────────────────────────

pub fn infer_step_type(name: &str) -> &str {
    let lower = name.to_lowercase();
    if lower.contains("source") || lower == "s" {
        "source"
    } else if lower.contains("target") || lower == "t" {
        "target"
    } else if lower.contains("transform") || lower.contains("map") {
        "transform"
    } else if lower.contains("condition") || lower.contains("check") || lower.contains("branch") {
        "condition"
    } else {
        "transform"
    }
}

async fn create_dag_step(world: &mut StepWorld, name: &str, step_type: &str) -> String {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id stored")
        .clone();

    let body = json!({
        "name": name,
        "step_type": step_type,
        "config": {}
    });

    let path = format!("/api/v1/pipes/{}/dag/steps", template_id);
    let (_status, _body) = world.post_json(&path, &body).await;

    let step_id = world
        .response_json
        .as_ref()
        .and_then(|j| j.pointer("/item/id"))
        .and_then(|v| v.as_str())
        .expect("Expected /item/id in step response")
        .to_string();

    world
        .stored_ids
        .insert(format!("dag_step:{}", name), step_id.clone());
    world
        .stored_ids
        .insert("last_dag_step_id".to_string(), step_id.clone());

    step_id
}

async fn create_dag_edge(world: &mut StepWorld, from_name: &str, to_name: &str) -> String {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let from_id = world
        .stored_ids
        .get(&format!("dag_step:{}", from_name))
        .expect(&format!("No step named '{}'", from_name))
        .clone();
    let to_id = world
        .stored_ids
        .get(&format!("dag_step:{}", to_name))
        .expect(&format!("No step named '{}'", to_name))
        .clone();

    let body = json!({
        "from_step_id": from_id,
        "to_step_id": to_id
    });

    let path = format!("/api/v1/pipes/{}/dag/edges", template_id);
    let (_status, _body) = world.post_json(&path, &body).await;

    let edge_id = world
        .response_json
        .as_ref()
        .and_then(|j| j.pointer("/item/id"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    world
        .stored_ids
        .insert("last_dag_edge_id".to_string(), edge_id.clone());

    edge_id
}

// ─── Given steps ──────────────────────────────────────────────

#[given(regex = r#"^I have added DAG steps "([^"]+)" to the template$"#)]
async fn given_dag_steps(world: &mut StepWorld, steps_csv: String) {
    for name in steps_csv.split(',') {
        let name = name.trim();
        let step_type = infer_step_type(name);
        create_dag_step(world, name, step_type).await;
    }
}

#[given(regex = r#"^I have added a DAG step "([^"]+)" of type "([^"]+)" to the template$"#)]
async fn given_dag_step_with_type(world: &mut StepWorld, name: String, step_type: String) {
    create_dag_step(world, &name, &step_type).await;
}

#[given(regex = r#"^I have added a DAG edge from step "([^"]+)" to step "([^"]+)"$"#)]
async fn given_dag_edge(world: &mut StepWorld, from_name: String, to_name: String) {
    create_dag_edge(world, &from_name, &to_name).await;
}

// ─── When steps ───────────────────────────────────────────────

#[when("I add a DAG step to the template with:")]
async fn when_add_dag_step(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();

    let docstring = step.docstring.as_ref().expect("Missing docstring");
    let body: serde_json::Value = serde_json::from_str(docstring).expect("Invalid JSON in docstring");

    let path = format!("/api/v1/pipes/{}/dag/steps", template_id);
    world.post_json(&path, &body).await;

    // Store the step ID if created
    if let Some(step_id) = world
        .response_json
        .as_ref()
        .and_then(|j| j.pointer("/item/id"))
        .and_then(|v| v.as_str())
    {
        world
            .stored_ids
            .insert("last_dag_step_id".to_string(), step_id.to_string());
    }
}

#[when("I list DAG steps for the template")]
async fn when_list_dag_steps(world: &mut StepWorld) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();

    let path = format!("/api/v1/pipes/{}/dag/steps", template_id);
    world.get(&path).await;
}

#[when("I get the stored DAG step")]
async fn when_get_dag_step(world: &mut StepWorld) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let step_id = world
        .stored_ids
        .get("last_dag_step_id")
        .expect("No last_dag_step_id")
        .clone();

    let path = format!("/api/v1/pipes/{}/dag/steps/{}", template_id, step_id);
    world.get(&path).await;
}

#[when("I update the stored DAG step with:")]
async fn when_update_dag_step(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let step_id = world
        .stored_ids
        .get("last_dag_step_id")
        .expect("No last_dag_step_id")
        .clone();

    let docstring = step.docstring.as_ref().expect("Missing docstring");
    let body: serde_json::Value = serde_json::from_str(docstring).expect("Invalid JSON");

    let path = format!("/api/v1/pipes/{}/dag/steps/{}", template_id, step_id);
    world.put_json(&path, &body).await;
}

#[when("I delete the stored DAG step")]
async fn when_delete_dag_step(world: &mut StepWorld) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let step_id = world
        .stored_ids
        .get("last_dag_step_id")
        .expect("No last_dag_step_id")
        .clone();

    let path = format!("/api/v1/pipes/{}/dag/steps/{}", template_id, step_id);
    world.delete(&path).await;
}

#[when(regex = r#"^I add a DAG edge from step "([^"]+)" to step "([^"]+)"$"#)]
async fn when_add_dag_edge(world: &mut StepWorld, from_name: String, to_name: String) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let from_id = world
        .stored_ids
        .get(&format!("dag_step:{}", from_name))
        .expect(&format!("No step named '{}'", from_name))
        .clone();
    let to_id = world
        .stored_ids
        .get(&format!("dag_step:{}", to_name))
        .expect(&format!("No step named '{}'", to_name))
        .clone();

    let body = json!({
        "from_step_id": from_id,
        "to_step_id": to_id
    });

    let path = format!("/api/v1/pipes/{}/dag/edges", template_id);
    world.post_json(&path, &body).await;

    if let Some(edge_id) = world
        .response_json
        .as_ref()
        .and_then(|j| j.pointer("/item/id"))
        .and_then(|v| v.as_str())
    {
        world
            .stored_ids
            .insert("last_dag_edge_id".to_string(), edge_id.to_string());
    }
}

#[when(regex = r#"^I add a DAG edge from step "([^"]+)" to step "([^"]+)" with condition:$"#)]
async fn when_add_dag_edge_with_condition(
    world: &mut StepWorld,
    step: &cucumber::gherkin::Step,
    from_name: String,
    to_name: String,
) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let from_id = world
        .stored_ids
        .get(&format!("dag_step:{}", from_name))
        .expect(&format!("No step named '{}'", from_name))
        .clone();
    let to_id = world
        .stored_ids
        .get(&format!("dag_step:{}", to_name))
        .expect(&format!("No step named '{}'", to_name))
        .clone();

    let docstring = step.docstring.as_ref().expect("Missing docstring");
    let condition: serde_json::Value = serde_json::from_str(docstring).expect("Invalid JSON");

    let body = json!({
        "from_step_id": from_id,
        "to_step_id": to_id,
        "condition": condition
    });

    let path = format!("/api/v1/pipes/{}/dag/edges", template_id);
    world.post_json(&path, &body).await;

    if let Some(edge_id) = world
        .response_json
        .as_ref()
        .and_then(|j| j.pointer("/item/id"))
        .and_then(|v| v.as_str())
    {
        world
            .stored_ids
            .insert("last_dag_edge_id".to_string(), edge_id.to_string());
    }
}

#[when("I list DAG edges for the template")]
async fn when_list_dag_edges(world: &mut StepWorld) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();

    let path = format!("/api/v1/pipes/{}/dag/edges", template_id);
    world.get(&path).await;
}

#[when("I delete the stored DAG edge")]
async fn when_delete_dag_edge(world: &mut StepWorld) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let edge_id = world
        .stored_ids
        .get("last_dag_edge_id")
        .expect("No last_dag_edge_id")
        .clone();

    let path = format!("/api/v1/pipes/{}/dag/edges/{}", template_id, edge_id);
    world.delete(&path).await;
}

#[when("I validate the DAG for the template")]
async fn when_validate_dag(world: &mut StepWorld) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();

    let path = format!("/api/v1/pipes/{}/dag/validate", template_id);
    let body = json!({});
    world.post_json(&path, &body).await;
}
