/// IDOR security tests for command endpoints.
///
/// Commands are scoped to a deployment_hash. These tests verify that User B
/// cannot read commands belonging to User A's deployments.
mod common;

use reqwest::StatusCode;

use tokio::sync::OnceCell;

static APP: OnceCell<common::TwoUserTestApp> = OnceCell::const_new();

async fn app() -> &'static common::TwoUserTestApp {
    common::get_or_init_two_user_app(&APP)
        .await
        .expect("Failed to start test app")
}

/// Seed a deployment and insert a command for the given user.
/// Returns (deployment_hash, command_id).
async fn seed_deployment_with_command(pool: &sqlx::PgPool, user_id: &str) -> (String, String) {
    let project_id = common::create_test_project(pool, user_id).await;
    let hash = format!("dpl-{}", uuid::Uuid::new_v4());
    let _deployment_id = common::create_test_deployment(pool, user_id, project_id, &hash).await;

    let command_id = format!("cmd-{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO commands (command_id, deployment_hash, type, status, parameters, created_by, created_at)
         VALUES ($1, $2, $3, 'queued', '{}'::jsonb, $4, NOW())",
    )
    .bind(&command_id)
    .bind(&hash)
    .bind("status")
    .bind(user_id)
    .execute(pool)
    .await
    .expect("Failed to insert test command");

    (hash, command_id)
}

// ── KNOWN VULNERABLE: list commands leaks across users ──────────────────

/// User B should NOT see commands for User A's deployment.
/// Currently the endpoint performs no ownership check on the deployment.
#[tokio::test]
async fn test_list_commands_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (hash_a, _cmd_id) = seed_deployment_with_command(&app.db_pool, common::USER_A_ID).await;

    let resp = client
        .get(format!("{}/api/v1/commands/{}", app.address, hash_a))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");

    // After fix this should be 404 or an empty list
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();

    if status == StatusCode::OK {
        let list = body["list"].as_array().expect("list should be an array");
        assert!(
            list.is_empty(),
            "User B should not see User A's commands (got {} items)",
            list.len()
        );
    } else {
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

// ── KNOWN VULNERABLE: get command detail leaks across users ─────────────

/// User B should NOT be able to fetch a specific command from User A's deployment.
#[tokio::test]
async fn test_get_command_detail_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (hash_a, cmd_id) = seed_deployment_with_command(&app.db_pool, common::USER_A_ID).await;

    let resp = client
        .get(format!(
            "{}/api/v1/commands/{}/{}",
            app.address, hash_a, cmd_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not read User A's command detail"
    );
}

// ── Positive: owner can list own commands ────────────────────────────────

#[tokio::test]
async fn test_owner_can_list_own_commands() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (hash_a, cmd_id) = seed_deployment_with_command(&app.db_pool, common::USER_A_ID).await;

    // List
    let resp = client
        .get(format!("{}/api/v1/commands/{}", app.address, hash_a))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("list should be an array");
    assert!(
        list.iter()
            .any(|c| c["command_id"].as_str() == Some(&cmd_id)),
        "Owner should see their own command in the list"
    );

    // Detail
    let resp = client
        .get(format!(
            "{}/api/v1/commands/{}/{}",
            app.address, hash_a, cmd_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["item"]["command_id"].as_str(), Some(cmd_id.as_str()));
}

// ── Create and cancel: the routes that change another user's server ──────

/// User B must not queue a command for User A's agent: a queued command runs
/// on User A's server.
#[tokio::test]
async fn test_create_command_rejects_other_users_deployment() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let (hash_a, _cmd_id) = seed_deployment_with_command(&app.db_pool, common::USER_A_ID).await;

    let resp = reqwest::Client::new()
        .post(format!("{}/api/v1/commands", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&serde_json::json!({
            "deployment_hash": hash_a,
            "command_type": "health",
            "parameters": { "app_code": "all" }
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not queue commands on User A's deployment"
    );
    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM commands WHERE deployment_hash = $1 AND created_by = $2",
    )
    .bind(&hash_a)
    .bind(common::USER_B_ID)
    .fetch_one(&app.db_pool)
    .await
    .unwrap();
    assert_eq!(queued, 0, "a command from User B reached User A's queue");
}

/// User B must not cancel User A's queued command.
#[tokio::test]
async fn test_cancel_command_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let (hash_a, cmd_id) = seed_deployment_with_command(&app.db_pool, common::USER_A_ID).await;

    let resp = reqwest::Client::new()
        .post(format!(
            "{}/api/v1/commands/{}/{}/cancel",
            app.address, hash_a, cmd_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not cancel User A's command"
    );
    let status: String = sqlx::query_scalar("SELECT status FROM commands WHERE command_id = $1")
        .bind(&cmd_id)
        .fetch_one(&app.db_pool)
        .await
        .unwrap();
    assert_eq!(status, "queued");
}

/// The owner cancels a queued command by the command_id the API hands out.
#[tokio::test]
async fn test_owner_can_cancel_own_queued_command() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let (hash_a, cmd_id) = seed_deployment_with_command(&app.db_pool, common::USER_A_ID).await;

    let resp = reqwest::Client::new()
        .post(format!(
            "{}/api/v1/commands/{}/{}/cancel",
            app.address, hash_a, cmd_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::OK, "owner could not cancel");
    let status: String = sqlx::query_scalar("SELECT status FROM commands WHERE command_id = $1")
        .bind(&cmd_id)
        .fetch_one(&app.db_pool)
        .await
        .unwrap();
    assert_eq!(status, "cancelled");
}

/// deploy_app takes the project to write the app config into from
/// `parameters.deployment_id`. Pointing it at User A's project from User B's
/// own deployment must not write into User A's project.
#[tokio::test]
async fn test_deploy_app_command_rejects_other_users_project_id() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let project_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let (hash_b, _cmd_id) = seed_deployment_with_command(&app.db_pool, common::USER_B_ID).await;

    let resp = reqwest::Client::new()
        .post(format!("{}/api/v1/commands", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&serde_json::json!({
            "deployment_hash": hash_b,
            "command_type": "deploy_app",
            "parameters": {
                "deployment_id": project_a,
                "app_code": "intruder",
                "image": "nginx:alpine",
                "parameters": { "image": "nginx:alpine" }
            }
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not target User A's project"
    );
    let apps: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_app WHERE project_id = $1")
        .bind(project_a)
        .fetch_one(&app.db_pool)
        .await
        .unwrap();
    assert_eq!(apps, 0, "User B wrote an app into User A's project");
}
