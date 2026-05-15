use cucumber::{given, then, when};
use serde_json::json;

use super::StepWorld;

// ─── Given: create pipe instance for DAG execution ────────────

#[given("I have a DAG pipe instance for that template")]
async fn given_dag_instance(world: &mut StepWorld) {
    let pool = world.db_pool.as_ref().expect("no db_pool");

    // Create a project + deployment for this instance
    let deployment_hash = format!("dag-exec-deploy-{}", uuid::Uuid::new_v4());
    let proj_name = format!("proj-{}", &deployment_hash);

    let project_id: i32 = sqlx::query_scalar(
        r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, created_at, updated_at)
           VALUES (gen_random_uuid(), $1, $2, '{}'::json, '{}'::json, NOW(), NOW())
           RETURNING id"#,
    )
    .bind(super::common::USER_A_ID)
    .bind(&proj_name)
    .fetch_one(pool)
    .await
    .expect("project insert failed");

    sqlx::query(
        r#"INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, created_at, updated_at)
           VALUES ($1, $2, $3, '{}'::json, 'running', NOW(), NOW())
           ON CONFLICT (deployment_hash) DO UPDATE SET updated_at = NOW()"#,
    )
    .bind(project_id)
    .bind(&deployment_hash)
    .bind(super::common::USER_A_ID)
    .execute(pool)
    .await
    .expect("deployment insert failed");

    // Create a pipe instance via the API
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();

    let body = json!({
        "template_id": template_id,
        "deployment_hash": deployment_hash,
        "source_container": "dag-source",
        "target_container": "dag-target"
    });

    let (_status, _body) = world.post_json("/api/v1/pipes/instances", &body).await;

    assert_eq!(
        world.status_code,
        Some(201),
        "Failed to create DAG pipe instance: {}",
        world.response_body.as_deref().unwrap_or("<none>")
    );

    world.store_id_from_response("dag_instance_id", "/item/id");
    world
        .stored_ids
        .insert("dag_deployment_hash".to_string(), deployment_hash);
}

// ─── Given: create step with config ───────────────────────────

#[given(regex = r#"^I have added a DAG step "([^"]+)" of type "([^"]+)" with config:$"#)]
async fn given_dag_step_with_config(
    world: &mut StepWorld,
    step: &cucumber::gherkin::Step,
    name: String,
    step_type: String,
) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id stored")
        .clone();

    let docstring = step.docstring.as_ref().expect("Missing docstring");
    let config: serde_json::Value = serde_json::from_str(docstring).expect("Invalid JSON");

    let body = json!({
        "name": name,
        "step_type": step_type,
        "config": config
    });

    let path = format!("/api/v1/pipes/{}/dag/steps", template_id);
    let (_status, _body) = world.post_json(&path, &body).await;

    assert_eq!(
        world.status_code,
        Some(201),
        "Failed to create DAG step '{}': {}",
        name,
        world.response_body.as_deref().unwrap_or("<none>")
    );

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
        .insert("last_dag_step_id".to_string(), step_id);
}

// ─── When: execute the DAG ────────────────────────────────────

#[when("I execute the DAG with input:")]
async fn when_execute_dag(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let instance_id = world
        .stored_ids
        .get("dag_instance_id")
        .expect("No dag_instance_id")
        .clone();

    let docstring = step.docstring.as_ref().expect("Missing docstring");
    let input: serde_json::Value = serde_json::from_str(docstring).expect("Invalid JSON");

    let body = json!({
        "input_data": input
    });

    let path = format!("/api/v1/pipes/instances/{}/dag/execute", instance_id);
    world.post_json(&path, &body).await;

    // Store execution_id for step execution queries
    if let Some(exec_id) = world
        .response_json
        .as_ref()
        .and_then(|j| j.pointer("/item/execution_id"))
        .and_then(|v| v.as_str())
    {
        world
            .stored_ids
            .insert("dag_execution_id".to_string(), exec_id.to_string());
    }
}

#[when("another user executes the DAG for the stored template")]
async fn when_other_user_executes_dag(world: &mut StepWorld) {
    let instance_id = world
        .stored_ids
        .get("dag_instance_id")
        .expect("No dag_instance_id")
        .clone();

    let body = json!({"input_data": {}});
    let path = format!("/api/v1/pipes/instances/{}/dag/execute", instance_id);

    // Use "user-b" token which maps to a different user in mock auth
    let resp = world
        .client
        .post(format!("{}{}", world.base_url, path))
        .header("Authorization", "Bearer user-b")
        .json(&body)
        .send()
        .await
        .expect("request failed");

    world.status_code = Some(resp.status().as_u16());
    let body_text = resp.text().await.unwrap_or_default();
    world.response_json = serde_json::from_str(&body_text).ok();
    world.response_body = Some(body_text);
}

// ─── When: list step executions ───────────────────────────────

#[when("I list step executions for the DAG execution")]
async fn when_list_step_executions(world: &mut StepWorld) {
    let template_id = world
        .stored_ids
        .get("dag_template_id")
        .expect("No dag_template_id")
        .clone();
    let execution_id = world
        .stored_ids
        .get("dag_execution_id")
        .expect("No dag_execution_id")
        .clone();

    let path = format!(
        "/api/v1/pipes/{}/dag/executions/{}/steps",
        template_id, execution_id
    );
    world.get(&path).await;
}

// ─── Then: execution result assertions ────────────────────────

#[then(regex = r#"^every step execution should have status "([^"]+)"$"#)]
async fn then_every_step_has_status(world: &mut StepWorld, expected_status: String) {
    let list = world
        .response_json
        .as_ref()
        .and_then(|j| j.get("list"))
        .and_then(|l| l.as_array())
        .expect("Expected /list array in response");

    for (i, item) in list.iter().enumerate() {
        let status = item
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        assert_eq!(
            status, expected_status,
            "Step execution {} has status '{}', expected '{}'",
            i, status, expected_status
        );
    }
}

#[then(regex = r#"^each step execution should have a "([^"]+)" field$"#)]
async fn then_each_step_has_field(world: &mut StepWorld, field: String) {
    let list = world
        .response_json
        .as_ref()
        .and_then(|j| j.get("list"))
        .and_then(|l| l.as_array())
        .expect("Expected /list array in response");

    for (i, item) in list.iter().enumerate() {
        assert!(
            item.get(&field).is_some(),
            "Step execution {} missing field '{}'",
            i,
            field
        );
    }
}

// ─── Then: response body contains substring ───────────────────

#[then(regex = r#"^the response body should contain "([^"]+)"$"#)]
async fn then_response_body_contains(world: &mut StepWorld, substring: String) {
    let body = world.response_body.as_deref().unwrap_or("");
    assert!(
        body.contains(&substring),
        "Expected response body to contain '{}', got: {}",
        substring,
        body
    );
}

// ─── Then: JSON array length ──────────────────────────────────

#[then(regex = r#"^the response JSON at "([^"]+)" should have length (\d+)$"#)]
async fn then_json_array_length(world: &mut StepWorld, path: String, expected: usize) {
    let json = world.response_json.as_ref().expect("No JSON response");
    let arr = json
        .pointer(&path)
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("JSON path '{}' not found or not array in: {}", path, json));
    assert_eq!(
        arr.len(),
        expected,
        "Expected array at '{}' to have length {}, got {}",
        path,
        expected,
        arr.len()
    );
}

// (shorthand for adding multiple steps is in dag.rs)
