/// IDOR security tests for deployment endpoints.
///
/// Verify that deployments are isolated per user — User B cannot read User A's data.
mod common;

use reqwest::StatusCode;

/// Helper: create a project + deployment for the given user, return (project_id, deployment_id, hash).
async fn seed_deployment(
    pool: &sqlx::PgPool,
    user_id: &str,
) -> (i32, i32, String) {
    let project_id = common::create_test_project(pool, user_id).await;
    let hash = format!("dpl-{}", uuid::Uuid::new_v4());
    let deployment_id =
        common::create_test_deployment(pool, user_id, project_id, &hash).await;
    (project_id, deployment_id, hash)
}

// ── List ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_deployments_only_returns_own() {
    let Some(app) = common::spawn_app_two_users().await else { return };
    let client = reqwest::Client::new();

    // Seed one deployment per user
    let (_pid_a, _did_a, _hash_a) = seed_deployment(&app.db_pool, common::USER_A_ID).await;
    let (_pid_b, _did_b, _hash_b) = seed_deployment(&app.db_pool, common::USER_B_ID).await;

    // User A lists — should see only their own
    let resp = client
        .get(format!("{}/api/v1/deployments", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("list should be an array");
    assert_eq!(list.len(), 1, "User A should see exactly 1 deployment");
    assert_eq!(list[0]["deployment_hash"].as_str().unwrap(), _hash_a);

    // User B lists — should see only their own
    let resp = client
        .get(format!("{}/api/v1/deployments", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("list should be an array");
    assert_eq!(list.len(), 1, "User B should see exactly 1 deployment");
    assert_eq!(list[0]["deployment_hash"].as_str().unwrap(), _hash_b);
}

// ── Get by ID ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_get_deployment_by_id_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else { return };
    let client = reqwest::Client::new();

    let (_pid, did, _hash) = seed_deployment(&app.db_pool, common::USER_A_ID).await;

    // User B tries to access User A's deployment by ID
    let resp = client
        .get(format!("{}/api/v1/deployments/{}", app.address, did))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not access User A's deployment by ID"
    );
}

// ── Get by hash ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_get_deployment_by_hash_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else { return };
    let client = reqwest::Client::new();

    let (_pid, _did, hash) = seed_deployment(&app.db_pool, common::USER_A_ID).await;

    let resp = client
        .get(format!("{}/api/v1/deployments/hash/{}", app.address, hash))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not access User A's deployment by hash"
    );
}

// ── Get by project ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_get_deployment_by_project_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else { return };
    let client = reqwest::Client::new();

    let (pid, _did, _hash) = seed_deployment(&app.db_pool, common::USER_A_ID).await;

    let resp = client
        .get(format!("{}/api/v1/deployments/project/{}", app.address, pid))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not access User A's deployment by project"
    );
}

// ── Positive: owner can access own ──────────────────────────────────────

#[tokio::test]
async fn test_owner_can_access_own_deployment() {
    let Some(app) = common::spawn_app_two_users().await else { return };
    let client = reqwest::Client::new();

    let (_pid, did, hash) = seed_deployment(&app.db_pool, common::USER_A_ID).await;

    // By ID
    let resp = client
        .get(format!("{}/api/v1/deployments/{}", app.address, did))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["item"]["deployment_hash"].as_str().unwrap(), hash);

    // By hash
    let resp = client
        .get(format!("{}/api/v1/deployments/hash/{}", app.address, hash))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), StatusCode::OK);
}
