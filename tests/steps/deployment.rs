use cucumber::{given, then, when};
use serde_json::json;

use super::StepWorld;

// ─── Deployment setup steps ──────────────────────────────────────

#[given(regex = r#"^I have a test deployment with hash "(.+)" and status "(.+)"$"#)]
async fn given_test_deployment_with_status(
    world: &mut StepWorld,
    deployment_hash: String,
    status: String,
) {
    let pool = world.db_pool.as_ref().expect("no db_pool");
    let proj_name = format!("proj-{}", &deployment_hash);

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

    let deploy_id = sqlx::query_scalar::<_, i32>(
        r#"INSERT INTO deployment (project_id, deployment_hash, user_id, metadata, status, created_at, updated_at)
           VALUES ($1, $2, $3, '{}'::json, $4, NOW(), NOW())
           ON CONFLICT (deployment_hash) DO UPDATE SET status = $4, updated_at = NOW()
           RETURNING id"#,
    )
    .bind(project_id)
    .bind(&deployment_hash)
    .bind(super::common::USER_A_ID)
    .bind(&status)
    .fetch_one(pool)
    .await
    .expect("Failed to create test deployment");

    world
        .stored_ids
        .insert("deployment_hash".to_string(), deployment_hash);
    world
        .stored_ids
        .insert("deployment_id".to_string(), deploy_id.to_string());
    world
        .stored_ids
        .insert("deployment_project_id".to_string(), project_id.to_string());
}

// ─── Deployment query steps ──────────────────────────────────────

#[when("I list deployments")]
async fn list_deployments(world: &mut StepWorld) {
    world.get("/api/v1/deployments").await;
}

#[when("I get the stored deployment by ID")]
async fn get_deployment_by_id(world: &mut StepWorld) {
    let deployment_id = world
        .stored_ids
        .get("deployment_id")
        .expect("No stored deployment_id")
        .clone();
    world
        .get(&format!("/api/v1/deployments/{}", deployment_id))
        .await;
}

#[when(regex = r#"^I get the deployment by hash "([^"]+)"$"#)]
async fn get_deployment_by_hash(world: &mut StepWorld, hash: String) {
    world
        .get(&format!("/api/v1/deployments/hash/{}", hash))
        .await;
}

#[when("I get the deployment by project")]
async fn get_deployment_by_project(world: &mut StepWorld) {
    let project_id = world
        .stored_ids
        .get("deployment_project_id")
        .expect("No stored deployment_project_id")
        .clone();
    world
        .get(&format!("/api/v1/deployments/project/{}", project_id))
        .await;
}

#[when("I force complete the stored deployment")]
async fn force_complete_deployment(world: &mut StepWorld) {
    let deployment_id = world
        .stored_ids
        .get("deployment_id")
        .expect("No stored deployment_id")
        .clone();
    let body = json!({});
    world
        .post_json(
            &format!("/api/v1/deployments/{}/force-complete", deployment_id),
            &body,
        )
        .await;
}

#[when(regex = r#"^I force complete deployment "([^"]+)"$"#)]
async fn force_complete_deployment_by_hash(world: &mut StepWorld, hash: String) {
    // Look up deployment ID from hash
    let pool = world.db_pool.as_ref().expect("no db_pool");
    let deploy_id: i32 =
        sqlx::query_scalar("SELECT id FROM deployment WHERE deployment_hash = $1 LIMIT 1")
            .bind(&hash)
            .fetch_one(pool)
            .await
            .expect("Deployment not found");
    let body = json!({});
    world
        .post_json(
            &format!("/api/v1/deployments/{}/force-complete", deploy_id),
            &body,
        )
        .await;
}

#[when(regex = r#"^I get capabilities for deployment "([^"]+)"$"#)]
async fn get_capabilities(world: &mut StepWorld, deployment_hash: String) {
    world
        .get(&format!(
            "/api/v1/deployments/{}/capabilities",
            deployment_hash
        ))
        .await;
}
