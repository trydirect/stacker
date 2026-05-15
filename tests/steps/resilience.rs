use cucumber::{given, when};
use serde_json::json;

use super::StepWorld;

// ─── Background: create a pipe template + instance for resilience tests ───

#[given(regex = r#"^I have a resilience pipe template named "([^"]+)"$"#)]
async fn given_resilience_template(world: &mut StepWorld, name: String) {
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
        "Failed to create resilience pipe template '{}': {}",
        unique_name,
        world.response_body.as_deref().unwrap_or("<none>")
    );

    world.store_id_from_response("resilience_template_id", "/item/id");
}

#[given("I have a resilience pipe instance for that template")]
async fn given_resilience_instance(world: &mut StepWorld) {
    let pool = world.db_pool.as_ref().expect("no db_pool");

    // Create a project + deployment for this instance
    let deployment_hash = format!("res-deploy-{}", uuid::Uuid::new_v4());
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
        .get("resilience_template_id")
        .expect("No resilience_template_id")
        .clone();

    let body = json!({
        "template_id": template_id,
        "deployment_hash": deployment_hash,
        "source_container": "app-source",
        "target_container": "app-target"
    });

    let (_status, _body) = world.post_json("/api/v1/pipes/instances", &body).await;

    assert_eq!(
        world.status_code,
        Some(201),
        "Failed to create resilience pipe instance: {}",
        world.response_body.as_deref().unwrap_or("<none>")
    );

    world.store_id_from_response("resilience_instance_id", "/item/id");
    world
        .stored_ids
        .insert("resilience_deployment_hash".to_string(), deployment_hash);
}

// ─── DLQ helpers ──────────────────────────────────────────────────

async fn create_dlq_entry(world: &mut StepWorld, max_retries: Option<i32>) {
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();

    let mut body = json!({
        "error": "Connection refused to target",
        "payload": {"data": "test"}
    });

    if let Some(max) = max_retries {
        body["max_retries"] = json!(max);
    } else {
        body["max_retries"] = json!(3);
    }

    let path = format!("/api/v1/pipes/instances/{}/dlq", instance_id);
    world.post_json(&path, &body).await;

    assert_eq!(
        world.status_code,
        Some(201),
        "Failed to create DLQ entry: {}",
        world.response_body.as_deref().unwrap_or("<none>")
    );

    world.store_id_from_response("dlq_entry_id", "/item/id");
}

#[given("I have a failed pipe execution for the instance")]
async fn given_failed_execution(_world: &mut StepWorld) {
    // No-op: we don't actually need an execution record — the DLQ entry
    // accepts an optional pipe_execution_id
}

#[given("I have a DLQ entry for the pipe instance")]
async fn given_dlq_entry(world: &mut StepWorld) {
    create_dlq_entry(world, None).await;
}

#[given(regex = r#"^I have a DLQ entry with max_retries (\d+) for the pipe instance$"#)]
async fn given_dlq_entry_with_max(world: &mut StepWorld, max: i32) {
    create_dlq_entry(world, Some(max)).await;
}

#[given("I discard the stored DLQ entry")]
async fn given_discard_dlq(world: &mut StepWorld) {
    let entry_id = world
        .stored_ids
        .get("dlq_entry_id")
        .expect("No dlq_entry_id")
        .clone();
    let path = format!("/api/v1/pipes/dlq/{}", entry_id);
    world.delete(&path).await;
}

// ─── DLQ When steps ───────────────────────────────────────────────

#[when("I list DLQ entries for the pipe instance")]
async fn when_list_dlq(world: &mut StepWorld) {
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();
    let path = format!("/api/v1/pipes/instances/{}/dlq", instance_id);
    world.get(&path).await;
}

#[when(regex = r#"^I push the failed execution to the DLQ with:$"#)]
async fn when_push_to_dlq(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();

    let docstring = step.docstring.as_ref().expect("Missing docstring");
    let body: serde_json::Value =
        serde_json::from_str(docstring).expect("Invalid JSON in docstring");

    let path = format!("/api/v1/pipes/instances/{}/dlq", instance_id);
    world.post_json(&path, &body).await;
}

#[when("I get the stored DLQ entry")]
async fn when_get_dlq(world: &mut StepWorld) {
    let entry_id = world
        .stored_ids
        .get("dlq_entry_id")
        .expect("No dlq_entry_id")
        .clone();
    let path = format!("/api/v1/pipes/dlq/{}", entry_id);
    world.get(&path).await;
}

#[when("I retry the stored DLQ entry")]
async fn when_retry_dlq(world: &mut StepWorld) {
    let entry_id = world
        .stored_ids
        .get("dlq_entry_id")
        .expect("No dlq_entry_id")
        .clone();
    let path = format!("/api/v1/pipes/dlq/{}/retry", entry_id);
    world.post_json(&path, &json!({})).await;
}

#[when("I discard the stored DLQ entry")]
async fn when_discard_dlq(world: &mut StepWorld) {
    let entry_id = world
        .stored_ids
        .get("dlq_entry_id")
        .expect("No dlq_entry_id")
        .clone();
    let path = format!("/api/v1/pipes/dlq/{}", entry_id);
    world.delete(&path).await;
}

// ─── Circuit Breaker When steps ───────────────────────────────────

#[when("I get the circuit breaker status for the pipe instance")]
async fn when_get_cb(world: &mut StepWorld) {
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();
    let path = format!("/api/v1/pipes/instances/{}/circuit-breaker", instance_id);
    world.get(&path).await;
}

#[when(regex = r#"^I update the circuit breaker config for the pipe instance with:$"#)]
async fn when_update_cb(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();

    let docstring = step.docstring.as_ref().expect("Missing docstring");
    let body: serde_json::Value = serde_json::from_str(docstring).expect("Invalid JSON");

    let path = format!("/api/v1/pipes/instances/{}/circuit-breaker", instance_id);
    world.put_json(&path, &body).await;
}

#[given(
    regex = r#"^the circuit breaker is configured with failure_threshold (\d+) for the pipe instance$"#
)]
async fn given_cb_configured(world: &mut StepWorld, threshold: i32) {
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();

    let body = json!({
        "failure_threshold": threshold,
        "recovery_timeout_seconds": 60,
        "half_open_max_requests": 3
    });

    let path = format!("/api/v1/pipes/instances/{}/circuit-breaker", instance_id);
    world.put_json(&path, &body).await;

    assert_eq!(
        world.status_code,
        Some(200),
        "Failed to configure circuit breaker: {}",
        world.response_body.as_deref().unwrap_or("<none>")
    );
}

#[when("I record a circuit breaker failure for the pipe instance")]
async fn when_record_failure(world: &mut StepWorld) {
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();
    let path = format!(
        "/api/v1/pipes/instances/{}/circuit-breaker/failure",
        instance_id
    );
    world.post_json(&path, &json!({})).await;
}

#[when("I record a circuit breaker success for the pipe instance")]
async fn when_record_success(world: &mut StepWorld) {
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();
    let path = format!(
        "/api/v1/pipes/instances/{}/circuit-breaker/success",
        instance_id
    );
    world.post_json(&path, &json!({})).await;
}

#[when("I reset the circuit breaker for the pipe instance")]
async fn when_reset_cb(world: &mut StepWorld) {
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();
    let path = format!(
        "/api/v1/pipes/instances/{}/circuit-breaker/reset",
        instance_id
    );
    world.post_json(&path, &json!({})).await;
}

#[given("the circuit breaker is in open state for the pipe instance")]
async fn given_cb_open(world: &mut StepWorld) {
    // Configure with threshold=1 and record 1 failure to open it
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();

    let body = json!({
        "failure_threshold": 1,
        "recovery_timeout_seconds": 60,
        "half_open_max_requests": 1
    });
    let path = format!("/api/v1/pipes/instances/{}/circuit-breaker", instance_id);
    world.put_json(&path, &body).await;

    let path = format!(
        "/api/v1/pipes/instances/{}/circuit-breaker/failure",
        instance_id
    );
    world.post_json(&path, &json!({})).await;

    // Verify it's open
    let cb_json = world.response_json.as_ref().expect("no response");
    let state = cb_json
        .pointer("/item/state")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        state, "open",
        "Circuit breaker should be open after threshold failures"
    );
}

#[given(regex = r#"^I have recorded (\d+) circuit breaker failures for the pipe instance$"#)]
async fn given_recorded_failures(world: &mut StepWorld, count: i32) {
    let instance_id = world
        .stored_ids
        .get("resilience_instance_id")
        .expect("No resilience_instance_id")
        .clone();
    let path = format!(
        "/api/v1/pipes/instances/{}/circuit-breaker/failure",
        instance_id
    );
    for _ in 0..count {
        world.post_json(&path, &json!({})).await;
    }
}
