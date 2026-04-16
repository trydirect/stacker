use cucumber::{given, when};
use serde_json::json;

use super::StepWorld;

// ─── Cloud steps ─────────────────────────────────────────────────

#[when(regex = r#"^I create a cloud with provider "([^"]+)" and token "([^"]+)"$"#)]
async fn create_cloud_with_token(world: &mut StepWorld, provider: String, _token: String) {
    let body = json!({
        "provider": provider,
        "save_token": false
    });
    world.post_json("/cloud", &body).await;
    store_cloud_id(world);
}

#[given(regex = r#"^I have created a cloud with provider "([^"]+)"$"#)]
async fn given_created_cloud(world: &mut StepWorld, provider: String) {
    let body = json!({
        "provider": provider,
        "save_token": false
    });
    world.post_json("/cloud", &body).await;
    store_cloud_id(world);
}

fn store_cloud_id(world: &mut StepWorld) {
    if let Some(ref json) = world.response_json {
        if let Some(id) = json.get("id").and_then(|v| v.as_i64()) {
            world
                .stored_ids
                .insert("cloud_id".to_string(), id.to_string());
        } else if let Some(id) = json.pointer("/item/id").and_then(|v| v.as_i64()) {
            world
                .stored_ids
                .insert("cloud_id".to_string(), id.to_string());
        }
    }
}

#[when("I list clouds")]
async fn list_clouds(world: &mut StepWorld) {
    world.get("/cloud").await;
}

#[when("I get the stored cloud")]
async fn get_stored_cloud(world: &mut StepWorld) {
    let cloud_id = world
        .stored_ids
        .get("cloud_id")
        .expect("No stored cloud_id")
        .clone();
    world.get(&format!("/cloud/{}", cloud_id)).await;
}

#[when(regex = r#"^I update the stored cloud with provider "([^"]+)"$"#)]
async fn update_stored_cloud(world: &mut StepWorld, provider: String) {
    let cloud_id = world
        .stored_ids
        .get("cloud_id")
        .expect("No stored cloud_id")
        .clone();
    // NOTE: user_id is required in body because CloudForm.into() calls
    // self.user_id.clone().unwrap() — omitting it causes a server panic.
    // This is a pre-existing bug: the update handler sets user_id from
    // auth context AFTER the conversion, but the conversion panics first.
    let body = json!({
        "provider": provider,
        "name": format!("bdd-updated-{}", provider),
        "user_id": "test-user-id",
        "save_token": false
    });
    world
        .put_json(&format!("/cloud/{}", cloud_id), &body)
        .await;
}

#[when("I delete the stored cloud")]
async fn delete_stored_cloud(world: &mut StepWorld) {
    let cloud_id = world
        .stored_ids
        .get("cloud_id")
        .expect("No stored cloud_id")
        .clone();
    world.delete(&format!("/cloud/{}", cloud_id)).await;
}

// ─── Server steps ────────────────────────────────────────────────

#[when("I list servers")]
async fn list_servers(world: &mut StepWorld) {
    world.get("/server").await;
}

#[when("I get servers for the stored project")]
async fn get_servers_for_project(world: &mut StepWorld) {
    let project_id = world
        .stored_ids
        .get("deployment_project_id")
        .expect("No stored deployment_project_id")
        .clone();
    world
        .get(&format!("/server/project/{}", project_id))
        .await;
}

#[given("I have a test server")]
async fn given_test_server(world: &mut StepWorld) {
    let pool = world.db_pool.as_ref().expect("no db_pool");

    // Create project first
    let project_id: i32 = sqlx::query_scalar(
        r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, created_at, updated_at)
           VALUES (gen_random_uuid(), $1, 'srv-test-proj', '{}'::json, '{}'::json, NOW(), NOW())
           RETURNING id"#,
    )
    .bind(super::common::USER_A_ID)
    .fetch_one(pool)
    .await
    .expect("Failed to create test project");

    // Create server
    let server_id: i32 = sqlx::query_scalar(
        r#"INSERT INTO server (project_id, user_id, srv_ip, created_at, updated_at)
           VALUES ($1, $2, '10.0.0.1', NOW(), NOW())
           RETURNING id"#,
    )
    .bind(project_id)
    .bind(super::common::USER_A_ID)
    .fetch_one(pool)
    .await
    .expect("Failed to create test server");

    world
        .stored_ids
        .insert("server_id".to_string(), server_id.to_string());
    world
        .stored_ids
        .insert("deployment_project_id".to_string(), project_id.to_string());
}

#[when("I get delete preview for the stored server")]
async fn get_delete_preview(world: &mut StepWorld) {
    let server_id = world
        .stored_ids
        .get("server_id")
        .expect("No stored server_id")
        .clone();
    world
        .get(&format!("/server/{}/delete-preview", server_id))
        .await;
}
