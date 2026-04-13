//! Security tests for agent-related endpoints.
//!
//! Validates that users can only enqueue commands and access data
//! for deployments they own.

mod common;

use common::{
    create_test_deployment, create_test_project, spawn_app_two_users,
    spawn_app_two_users_with_user_service, USER_A_ID, USER_A_TOKEN, USER_B_TOKEN,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Helper: insert a command directly into the DB for testing.
async fn insert_test_command(
    pool: &sqlx::PgPool,
    deployment_hash: &str,
    created_by: &str,
) -> String {
    let cmd_id = format!("cmd_{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO commands (command_id, deployment_hash, type, status, parameters, created_by, created_at)
         VALUES ($1, $2, 'status', 'queued', '{}'::jsonb, $3, NOW())",
    )
    .bind(&cmd_id)
    .bind(deployment_hash)
    .bind(created_by)
    .execute(pool)
    .await
    .expect("Failed to insert test command");
    cmd_id
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Enqueue — User B should NOT enqueue on User A's deployment
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_enqueue_command_rejects_other_user() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // User A owns a deployment
    let project_id = create_test_project(&app.db_pool, USER_A_ID).await;
    let _dep_id = create_test_deployment(&app.db_pool, USER_A_ID, project_id, "dep-a-001").await;

    // User B tries to enqueue a command on User A's deployment
    let resp = client
        .post(format!("{}/api/v1/agent/commands/enqueue", &app.address))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .json(&serde_json::json!({
            "deployment_hash": "dep-a-001",
            "command_type": "status",
        }))
        .send()
        .await
        .expect("Failed to send request");

    // Should be 403 or 404, not 201
    assert!(
        resp.status() == 403 || resp.status() == 404,
        "User B should NOT enqueue on User A's deployment. Got: {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_owner_can_enqueue_on_own_deployment() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let project_id = create_test_project(&app.db_pool, USER_A_ID).await;
    let _dep_id = create_test_deployment(&app.db_pool, USER_A_ID, project_id, "dep-own-001").await;

    let resp = client
        .post(format!("{}/api/v1/agent/commands/enqueue", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .json(&serde_json::json!({
            "deployment_hash": "dep-own-001",
            "command_type": "status",
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        resp.status(),
        201,
        "Owner should be able to enqueue. Got: {}",
        resp.status()
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Commands list — User B should NOT list User A's commands
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_list_commands_rejects_other_user() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let project_id = create_test_project(&app.db_pool, USER_A_ID).await;
    let _dep_id = create_test_deployment(&app.db_pool, USER_A_ID, project_id, "dep-cmd-a").await;
    let _cmd_id = insert_test_command(&app.db_pool, "dep-cmd-a", USER_A_ID).await;

    // User B tries to list User A's commands
    let resp = client
        .get(format!("{}/api/v1/commands/dep-cmd-a", &app.address))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    // Should be 404 or empty, not return User A's commands
    if resp.status().is_success() {
        let body: serde_json::Value = resp.json().await.unwrap();
        let list = body["list"].as_array().expect("Expected list field");
        assert!(
            list.is_empty(),
            "User B should NOT see User A's commands. Got {} commands",
            list.len()
        );
    }
}

#[tokio::test]
async fn test_get_command_detail_rejects_other_user() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let project_id = create_test_project(&app.db_pool, USER_A_ID).await;
    let _dep_id = create_test_deployment(&app.db_pool, USER_A_ID, project_id, "dep-cmd-b").await;
    let cmd_id = insert_test_command(&app.db_pool, "dep-cmd-b", USER_A_ID).await;

    // User B tries to get User A's command detail
    let resp = client
        .get(format!(
            "{}/api/v1/commands/dep-cmd-b/{}",
            &app.address, cmd_id
        ))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        resp.status() == 403 || resp.status() == 404,
        "User B should NOT see User A's command detail. Got: {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_owner_can_list_own_commands() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let project_id = create_test_project(&app.db_pool, USER_A_ID).await;
    let _dep_id = create_test_deployment(&app.db_pool, USER_A_ID, project_id, "dep-cmd-own").await;
    let _cmd_id = insert_test_command(&app.db_pool, "dep-cmd-own", USER_A_ID).await;

    let resp = client
        .get(format!("{}/api/v1/commands/dep-cmd-own", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert!(resp.status().is_success(), "Owner should list own commands");
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("Expected list field");
    assert!(!list.is_empty(), "Owner should see at least one command");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Unauthenticated access should be rejected
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_enqueue_rejects_unauthenticated() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/v1/agent/commands/enqueue", &app.address))
        // No Authorization header
        .json(&serde_json::json!({
            "deployment_hash": "dep-test",
            "command_type": "status",
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        resp.status(),
        401,
        "Unauthenticated enqueue should be 401. Got: {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_commands_list_rejects_unauthenticated() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/v1/commands/some-hash", &app.address))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        resp.status(),
        401,
        "Unauthenticated command list should be 401. Got: {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_owner_can_enqueue_and_list_commands_for_legacy_installation_hash() {
    let user_service = MockServer::start().await;
    let auth_header = format!("Bearer {}", USER_A_TOKEN);
    Mock::given(method("GET"))
        .and(path("/api/1.0/installations"))
        .and(header("authorization", auth_header.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "_items": [{
                "_id": 13830,
                "stack_code": "openclaw",
                "status": "completed",
                "cloud": "hetzner",
                "deployment_hash": "legacy-dep-13830",
                "domain": "openclawtest1.com",
                "_created": "2026-04-13T10:00:00Z",
                "_updated": "2026-04-13T10:05:00Z"
            }]
        })))
        .mount(&user_service)
        .await;

    let Some(app) = spawn_app_two_users_with_user_service(&user_service.uri()).await else {
        return;
    };
    let client = reqwest::Client::new();

    let enqueue = client
        .post(format!("{}/api/v1/agent/commands/enqueue", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .json(&serde_json::json!({
            "deployment_hash": "legacy-dep-13830",
            "command_type": "status"
        }))
        .send()
        .await
        .expect("Failed to enqueue legacy command");

    assert_eq!(enqueue.status(), 201);

    let list = client
        .get(format!("{}/api/v1/commands/legacy-dep-13830", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to list legacy commands");

    assert!(
        list.status().is_success(),
        "Owner should list legacy commands"
    );
    let body: serde_json::Value = list.json().await.expect("List response should be json");
    let commands = body["list"].as_array().expect("Expected list field");
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["deployment_hash"], "legacy-dep-13830");
}
