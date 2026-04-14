use cucumber::{given, then, when};
use serde_json::json;

use super::StepWorld;

// ─── Deployment helper ───────────────────────────────────────────

#[given(regex = r#"^I have a test deployment with hash "(.+)"$"#)]
async fn given_test_deployment(world: &mut StepWorld, deployment_hash: String) {
    let pool = world.db_pool.as_ref().expect("no db_pool");
    let proj_name = format!("proj-{}", &deployment_hash);

    // Create a project first, then a deployment
    let project_id: i32 = sqlx::query_scalar(
        r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, created_at, updated_at)
           VALUES (gen_random_uuid(), $1, $2, '{}'::json, '{}'::json, NOW(), NOW())
           ON CONFLICT DO NOTHING
           RETURNING id"#,
    )
    .bind(super::common::USER_A_ID)
    .bind(&proj_name)
    .fetch_optional(pool)
    .await
    .expect("project insert query failed")
    .unwrap_or(0);

    let project_id = if project_id == 0 {
        sqlx::query_scalar::<_, i32>("SELECT id FROM project WHERE name = $1 LIMIT 1")
            .bind(&proj_name)
            .fetch_one(pool)
            .await
            .expect("project should exist")
    } else {
        project_id
    };

    // Insert deployment if not exists
    let _deploy_id = sqlx::query_scalar::<_, i32>(
        r#"INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, created_at, updated_at)
           VALUES ($1, $2, $3, '{}'::json, 'running', NOW(), NOW())
           ON CONFLICT (deployment_hash) DO UPDATE SET updated_at = NOW()
           RETURNING id"#,
    )
    .bind(project_id)
    .bind(&deployment_hash)
    .bind(super::common::USER_A_ID)
    .fetch_one(pool)
    .await
    .expect("Failed to create test deployment");

    world
        .stored_ids
        .insert("deployment_hash".to_string(), deployment_hash);
}

// ─── Pipe Template steps ─────────────────────────────────────────

fn pipe_template_body(name: &str, is_public: bool) -> serde_json::Value {
    json!({
        "name": name,
        "source_app_type": "wordpress",
        "source_endpoint": { "path": "/api/posts", "method": "GET" },
        "target_app_type": "mailchimp",
        "target_endpoint": { "path": "/api/hook", "method": "POST" },
        "field_mapping": { "email": "$.user_email", "name": "$.display_name" },
        "is_public": is_public
    })
}

#[when(regex = r#"^I create a pipe template "(.+)"$"#)]
async fn create_pipe_template(world: &mut StepWorld, name: String) {
    let body = pipe_template_body(&name, false);
    world.post_json("/api/v1/pipes/templates", &body).await;
    if world.status_code == Some(201) {
        world.store_id_from_response("template_id", "/item/id");
    }
}

#[given(regex = r#"^I have created a pipe template "(.+)"$"#)]
async fn given_pipe_template(world: &mut StepWorld, name: String) {
    let body = pipe_template_body(&name, false);
    world.post_json("/api/v1/pipes/templates", &body).await;
    assert_eq!(
        world.status_code,
        Some(201),
        "Failed to create pipe template '{}': {}",
        name,
        world.response_body.as_deref().unwrap_or("<none>")
    );
    world.store_id_from_response("template_id", "/item/id");
}

#[given(regex = r#"^I have created a public pipe template "(.+)"$"#)]
async fn given_public_pipe_template(world: &mut StepWorld, name: String) {
    let body = pipe_template_body(&name, true);
    world.post_json("/api/v1/pipes/templates", &body).await;
    assert_eq!(
        world.status_code,
        Some(201),
        "Failed to create public pipe template '{}': {}",
        name,
        world.response_body.as_deref().unwrap_or("<none>")
    );
    world.store_id_from_response("template_id", "/item/id");
}

#[when("I create a pipe template with empty name")]
async fn create_template_empty_name(world: &mut StepWorld) {
    let mut body = pipe_template_body("", false);
    body["name"] = json!("");
    world.post_json("/api/v1/pipes/templates", &body).await;
}

#[when("I create a pipe template with empty source_app_type")]
async fn create_template_empty_source(world: &mut StepWorld) {
    let mut body = pipe_template_body("test", false);
    body["source_app_type"] = json!("");
    world.post_json("/api/v1/pipes/templates", &body).await;
}

#[when("I create a pipe template with empty target_app_type")]
async fn create_template_empty_target(world: &mut StepWorld) {
    let mut body = pipe_template_body("test", false);
    body["target_app_type"] = json!("");
    world.post_json("/api/v1/pipes/templates", &body).await;
}

#[when("I list pipe templates")]
async fn list_pipe_templates(world: &mut StepWorld) {
    world.get("/api/v1/pipes/templates").await;
}

#[when("I get the stored pipe template")]
async fn get_stored_pipe_template(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    world.get(&format!("/api/v1/pipes/templates/{}", id)).await;
}

#[when("I delete the stored pipe template")]
async fn delete_stored_pipe_template(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    world
        .delete(&format!("/api/v1/pipes/templates/{}", id))
        .await;
}

// ─── Pipe Instance steps ─────────────────────────────────────────

#[when(regex = r#"^I create a pipe instance for deployment "(.+)" with source "(.+)" and target container "(.+)"$"#)]
#[given(regex = r#"^I have created a pipe instance for deployment "(.+)" with source "(.+)" and target container "(.+)"$"#)]
async fn create_pipe_instance_container(
    world: &mut StepWorld,
    deployment_hash: String,
    source: String,
    target: String,
) {
    let body = json!({
        "deployment_hash": deployment_hash,
        "source_container": source,
        "target_container": target
    });
    world.post_json("/api/v1/pipes/instances", &body).await;
    if world.status_code == Some(201) {
        world.store_id_from_response("instance_id", "/item/id");
    }
}

#[when(regex = r#"^I create a pipe instance for deployment "(.+)" with source "(.+)" and target url "(.+)"$"#)]
async fn create_pipe_instance_url(
    world: &mut StepWorld,
    deployment_hash: String,
    source: String,
    target_url: String,
) {
    let body = json!({
        "deployment_hash": deployment_hash,
        "source_container": source,
        "target_url": target_url
    });
    world.post_json("/api/v1/pipes/instances", &body).await;
    if world.status_code == Some(201) {
        world.store_id_from_response("instance_id", "/item/id");
    }
}

#[when(regex = r#"^I create a pipe instance for deployment "(.+)" with source "(.+)" and target container "(.+)" linked to the stored template$"#)]
async fn create_pipe_instance_with_template(
    world: &mut StepWorld,
    deployment_hash: String,
    source: String,
    target: String,
) {
    let template_id = world
        .stored_ids
        .get("template_id")
        .expect("No stored template_id")
        .clone();
    let body = json!({
        "deployment_hash": deployment_hash,
        "source_container": source,
        "target_container": target,
        "template_id": template_id
    });
    world.post_json("/api/v1/pipes/instances", &body).await;
    if world.status_code == Some(201) {
        world.store_id_from_response("instance_id", "/item/id");
    }
}

#[when("I create a pipe instance with empty deployment hash")]
async fn create_instance_empty_deployment(world: &mut StepWorld) {
    let body = json!({
        "deployment_hash": "",
        "source_container": "src",
        "target_container": "tgt"
    });
    world.post_json("/api/v1/pipes/instances", &body).await;
}

#[when("I create a pipe instance with empty source container")]
async fn create_instance_empty_source(world: &mut StepWorld) {
    let body = json!({
        "deployment_hash": "some-hash",
        "source_container": "",
        "target_container": "tgt"
    });
    world.post_json("/api/v1/pipes/instances", &body).await;
}

#[when("I create a pipe instance with no target")]
async fn create_instance_no_target(world: &mut StepWorld) {
    let body = json!({
        "deployment_hash": "some-hash",
        "source_container": "src"
    });
    world.post_json("/api/v1/pipes/instances", &body).await;
}

#[when(regex = r#"^I list pipe instances for deployment "(.+)"$"#)]
async fn list_pipe_instances(world: &mut StepWorld, deployment_hash: String) {
    world
        .get(&format!("/api/v1/pipes/instances/{}", deployment_hash))
        .await;
}

#[when("I get the stored pipe instance")]
async fn get_stored_pipe_instance(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("instance_id")
        .expect("No stored instance_id")
        .clone();
    world
        .get(&format!("/api/v1/pipes/instances/detail/{}", id))
        .await;
}

#[when(regex = r#"^I update the stored pipe instance status to "(.+)"$"#)]
async fn update_instance_status(world: &mut StepWorld, status: String) {
    let id = world
        .stored_ids
        .get("instance_id")
        .expect("No stored instance_id")
        .clone();
    let body = json!({ "status": status });
    world
        .put_json(&format!("/api/v1/pipes/instances/{}/status", id), &body)
        .await;
}

#[when("I delete the stored pipe instance")]
async fn delete_stored_pipe_instance(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("instance_id")
        .expect("No stored instance_id")
        .clone();
    world
        .delete(&format!("/api/v1/pipes/instances/{}", id))
        .await;
}

// ─── Pipe Execution steps ────────────────────────────────────────

#[when("I list executions for the stored pipe instance")]
async fn list_executions(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("instance_id")
        .expect("No stored instance_id")
        .clone();
    world
        .get(&format!("/api/v1/pipes/instances/{}/executions", id))
        .await;
}

#[when(regex = r#"^I get pipe execution "(.+)"$"#)]
async fn get_pipe_execution(world: &mut StepWorld, execution_id: String) {
    world
        .get(&format!("/api/v1/pipes/executions/{}", execution_id))
        .await;
}
