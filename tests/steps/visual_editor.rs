use cucumber::{then, when};
use serde_json::json;

use crate::steps::StepWorld;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Editor-specific step definitions
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[when(regex = r#"^I add a step "([^"]+)" of type "([^"]+)" at position (\d+),(\d+)$"#)]
async fn when_add_step_at_position(
    world: &mut StepWorld,
    name: String,
    step_type: String,
    _x: i32,
    _y: i32,
) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let body = json!({
        "name": name,
        "step_type": step_type,
        "config": {},
    });

    let (_status, _body_str) = world
        .post_json(&format!("/api/v1/pipes/{}/dag/steps", template_id), &body)
        .await;

    // Extract step ID from /item/id (JsonResponse wrapper)
    if let Some(ref json) = world.response_json {
        if let Some(id) = json.pointer("/item/id").and_then(|v| v.as_str()) {
            world
                .stored_ids
                .insert("last_dag_step_id".to_string(), id.to_string());
            world
                .stored_ids
                .insert(format!("dag_step:{}", name), id.to_string());
        }
    }
}

#[then(regex = r#"^the step should have name "([^"]+)"$"#)]
async fn then_step_has_name(world: &mut StepWorld, name: String) {
    let json = world.response_json.as_ref().expect("No response JSON");
    let actual = json
        .pointer("/item/name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert_eq!(actual, name);
}

#[then(regex = r#"^the step should have type "([^"]+)"$"#)]
async fn then_step_has_type(world: &mut StepWorld, step_type: String) {
    let json = world.response_json.as_ref().expect("No response JSON");
    let actual = json
        .pointer("/item/step_type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert_eq!(actual, step_type);
}

#[when(regex = r#"^I update the step name to "([^"]+)" with config:$"#)]
async fn when_update_step(world: &mut StepWorld, name: String, step: &cucumber::gherkin::Step) {
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
    let config: serde_json::Value =
        serde_json::from_str(step.docstring.as_ref().expect("Missing docstring"))
            .expect("Invalid JSON");

    let body = json!({ "name": name, "config": config });
    let (status, body_str) = world
        .put_json(
            &format!("/api/v1/pipes/{}/dag/steps/{}", template_id, step_id),
            &body,
        )
        .await;
    world.status_code = Some(status);
    world.response_body = Some(body_str);
}

#[when("I delete the step")]
async fn when_delete_step(world: &mut StepWorld) {
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
    let (status, body_str) = world
        .delete(&format!(
            "/api/v1/pipes/{}/dag/steps/{}",
            template_id, step_id
        ))
        .await;
    world.status_code = Some(status);
    world.response_body = Some(body_str);
}

#[then(regex = r#"^listing steps should return (\d+) steps?$"#)]
async fn then_listing_steps_count(world: &mut StepWorld, count: usize) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let (_status, body) = world
        .get(&format!("/api/v1/pipes/{}/dag/steps", template_id))
        .await;
    // Response is {"list": [...], "message": "..."}
    let list_json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let steps = list_json
        .get("list")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(steps, count, "Expected {} steps, got {}", count, steps);
}

#[when(regex = r#"^I add an edge from step "([^"]+)" to step "([^"]+)"$"#)]
async fn when_add_edge(world: &mut StepWorld, from: String, to: String) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let from_id = world
        .stored_ids
        .get(&format!("dag_step:{}", from))
        .expect(&format!("No step '{}'", from))
        .clone();
    let to_id = world
        .stored_ids
        .get(&format!("dag_step:{}", to))
        .expect(&format!("No step '{}'", to))
        .clone();

    let body = json!({ "from_step_id": from_id, "to_step_id": to_id });
    let (_status, _body_str) = world
        .post_json(&format!("/api/v1/pipes/{}/dag/edges", template_id), &body)
        .await;

    if let Some(ref json) = world.response_json {
        if let Some(id) = json.pointer("/item/id").and_then(|v| v.as_str()) {
            world
                .stored_ids
                .insert(format!("dag_edge:{}_{}", from, to), id.to_string());
            world
                .stored_ids
                .insert("last_dag_edge_id".to_string(), id.to_string());
        }
    }
}

#[then(regex = r#"^listing edges should return (\d+) edges?$"#)]
async fn then_listing_edges_count(world: &mut StepWorld, count: usize) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let (_status, body) = world
        .get(&format!("/api/v1/pipes/{}/dag/edges", template_id))
        .await;
    let list_json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let edges = list_json
        .get("list")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(edges, count, "Expected {} edges, got {}", count, edges);
}

#[when(regex = r#"^I delete the edge from "([^"]+)" to "([^"]+)"$"#)]
async fn when_delete_edge(world: &mut StepWorld, _from: String, _to: String) {
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
    let (status, body_str) = world
        .delete(&format!(
            "/api/v1/pipes/{}/dag/edges/{}",
            template_id, edge_id
        ))
        .await;
    world.status_code = Some(status);
    world.response_body = Some(body_str);
}

#[when("I validate the DAG")]
async fn when_validate_dag(world: &mut StepWorld) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let (status, body_str) = world
        .post_json(
            &format!("/api/v1/pipes/{}/dag/validate", template_id),
            &json!({}),
        )
        .await;
    world.status_code = Some(status);
    world.response_body = Some(body_str);
}

#[when("I add steps of all supported types")]
async fn when_add_all_step_types(world: &mut StepWorld) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let types = [
        "source",
        "transform",
        "condition",
        "target",
        "parallel_split",
        "parallel_join",
        "ws_source",
        "ws_target",
        "http_stream_source",
        "grpc_source",
        "grpc_target",
        "cdc_source",
    ];

    let mut created = 0;
    for (_i, step_type) in types.iter().enumerate() {
        let body = json!({
            "name": step_type,
            "step_type": step_type,
            "config": {},
        });
        let (status, _) = world
            .post_json(&format!("/api/v1/pipes/{}/dag/steps", template_id), &body)
            .await;
        if status == 200 || status == 201 {
            created += 1;
        }
    }
    world
        .stored_ids
        .insert("created_step_count".to_string(), created.to_string());
    world.status_code = Some(200);
}

#[then(regex = r#"^all (\d+) steps should be created successfully$"#)]
async fn then_all_steps_created(world: &mut StepWorld, expected: usize) {
    let count: usize = world
        .stored_ids
        .get("created_step_count")
        .expect("No created count")
        .parse()
        .unwrap();
    assert_eq!(
        count, expected,
        "Expected {} steps created, got {}",
        expected, count
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// V2: Unauthenticated access (Casbin anonymous)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[when(regex = r#"^I make an unauthenticated GET request to "([^"]+)"$"#)]
async fn when_unauthenticated_get(world: &mut StepWorld, path: String) {
    let (status, body) = world.get_unauthenticated(&path).await;
    world.status_code = Some(status);
    world.response_body = Some(body);
}

#[then(regex = r#"^the response status should not be (\d+)$"#)]
async fn then_status_not(world: &mut StepWorld, forbidden_status: u16) {
    let actual = world.status_code.expect("No status code recorded");
    assert_ne!(
        actual, forbidden_status,
        "Expected status NOT {}, but got {}",
        forbidden_status, actual
    );
}
