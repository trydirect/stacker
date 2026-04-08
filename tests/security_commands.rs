/// IDOR security tests for command endpoints.
///
/// Commands are scoped to a deployment_hash. These tests verify that User B
/// cannot read commands belonging to User A's deployments.
mod common;

use reqwest::StatusCode;

/// Seed a deployment and insert a command for the given user.
/// Returns (deployment_hash, command_id).
async fn seed_deployment_with_command(
    pool: &sqlx::PgPool,
    user_id: &str,
) -> (String, String) {
    let project_id = common::create_test_project(pool, user_id).await;
    let hash = format!("dpl-{}", uuid::Uuid::new_v4());
    let _deployment_id =
        common::create_test_deployment(pool, user_id, project_id, &hash).await;

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
    let Some(app) = common::spawn_app_two_users().await else { return };
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
    let Some(app) = common::spawn_app_two_users().await else { return };
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
    let Some(app) = common::spawn_app_two_users().await else { return };
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
        list.iter().any(|c| c["command_id"].as_str() == Some(&cmd_id)),
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
