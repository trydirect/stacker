/// IDOR security tests for pipe template and pipe instance endpoints.
///
/// Pipe templates have `is_public` and `created_by` columns.
/// Pipe instances are tied to a deployment_hash and `created_by`.
/// These tests verify that private data is not leaked across users.
mod common;

use reqwest::StatusCode;
use sqlx::Row;

/// Insert a private pipe template for the given user. Returns its UUID.
async fn seed_pipe_template(pool: &sqlx::PgPool, user_id: &str) -> uuid::Uuid {
    let name = format!("test-tmpl-{}", uuid::Uuid::new_v4());
    let row = sqlx::query(
        "INSERT INTO pipe_templates (name, source_app_type, source_endpoint, target_app_type, target_endpoint, field_mapping, is_public, created_by)
         VALUES ($1, 'app-a', '{\"path\":\"/api\"}'::jsonb, 'app-b', '{\"path\":\"/api\"}'::jsonb, '{}'::jsonb, false, $2)
         RETURNING id",
    )
    .bind(&name)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("Failed to insert test pipe template");

    row.get::<uuid::Uuid, _>("id")
}

/// Seed a deployment + pipe instance for the given user. Returns (deployment_hash, instance_id).
async fn seed_pipe_instance(
    pool: &sqlx::PgPool,
    user_id: &str,
) -> (String, uuid::Uuid) {
    let project_id = common::create_test_project(pool, user_id).await;
    let hash = format!("dpl-{}", uuid::Uuid::new_v4());
    let _did = common::create_test_deployment(pool, user_id, project_id, &hash).await;

    let row = sqlx::query(
        "INSERT INTO pipe_instances (deployment_hash, source_container, status, created_by)
         VALUES ($1, 'my-app', 'active', $2)
         RETURNING id",
    )
    .bind(&hash)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("Failed to insert test pipe instance");

    let instance_id = row.get::<uuid::Uuid, _>("id");
    (hash, instance_id)
}

// ── KNOWN VULNERABLE: list templates returns all (no user filter) ───────

/// User B should NOT see User A's private templates.
/// Currently the endpoint returns all templates regardless of ownership.
#[tokio::test]
async fn test_list_pipe_templates_leaks_all() {
    let Some(app) = common::spawn_app_two_users().await else { return };
    let client = reqwest::Client::new();

    let tmpl_id = seed_pipe_template(&app.db_pool, common::USER_A_ID).await;

    // User B lists templates (not requesting public_only)
    let resp = client
        .get(format!("{}/api/v1/pipes/templates", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("list should be an array");

    let ids: Vec<&str> = list
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();

    assert!(
        !ids.contains(&tmpl_id.to_string().as_str()),
        "User B should not see User A's private template {} in the list (found {} templates)",
        tmpl_id,
        list.len()
    );
}

// ── KNOWN VULNERABLE: get template ignores _user ────────────────────────

/// User B should NOT be able to fetch User A's private template by ID.
#[tokio::test]
async fn test_get_pipe_template_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else { return };
    let client = reqwest::Client::new();

    let tmpl_id = seed_pipe_template(&app.db_pool, common::USER_A_ID).await;

    let resp = client
        .get(format!(
            "{}/api/v1/pipes/templates/{}",
            app.address, tmpl_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not read User A's private pipe template"
    );
}

// ── KNOWN VULNERABLE: list instances has no user check ──────────────────

/// User B should NOT see pipe instances for User A's deployment.
#[tokio::test]
async fn test_list_pipe_instances_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else { return };
    let client = reqwest::Client::new();

    let (hash_a, _inst_id) = seed_pipe_instance(&app.db_pool, common::USER_A_ID).await;

    let resp = client
        .get(format!(
            "{}/api/v1/pipes/instances/{}",
            app.address, hash_a
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();

    if status == StatusCode::OK {
        let list = body["list"].as_array().expect("list should be an array");
        assert!(
            list.is_empty(),
            "User B should not see User A's pipe instances (got {} items)",
            list.len()
        );
    } else {
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

// ── KNOWN VULNERABLE: get instance ignores _user ────────────────────────

/// User B should NOT be able to fetch User A's pipe instance by ID.
#[tokio::test]
async fn test_get_pipe_instance_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else { return };
    let client = reqwest::Client::new();

    let (_hash_a, inst_id) = seed_pipe_instance(&app.db_pool, common::USER_A_ID).await;

    let resp = client
        .get(format!(
            "{}/api/v1/pipes/instances/detail/{}",
            app.address, inst_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not read User A's pipe instance"
    );
}

// ── Positive: owner can list own pipe instances ─────────────────────────

#[tokio::test]
async fn test_owner_can_list_own_pipe_instances() {
    let Some(app) = common::spawn_app_two_users().await else { return };
    let client = reqwest::Client::new();

    let (hash_a, inst_id) = seed_pipe_instance(&app.db_pool, common::USER_A_ID).await;

    // List
    let resp = client
        .get(format!(
            "{}/api/v1/pipes/instances/{}",
            app.address, hash_a
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("list should be an array");
    assert!(
        list.iter()
            .any(|i| i["id"].as_str() == Some(&inst_id.to_string())),
        "Owner should see their own pipe instance in the list"
    );

    // Detail
    let resp = client
        .get(format!(
            "{}/api/v1/pipes/instances/detail/{}",
            app.address, inst_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::OK);
}
