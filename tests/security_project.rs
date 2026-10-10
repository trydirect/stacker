mod common;

use tokio::sync::OnceCell;

static APP: OnceCell<common::TwoUserTestApp> = OnceCell::const_new();

async fn app() -> &'static common::TwoUserTestApp {
    common::get_or_init_two_user_app(&APP)
        .await
        .expect("Failed to start test app")
}

/// IDOR security tests for /project endpoints.
/// Verify that User B cannot list, read, update, or delete User A's projects.
#[tokio::test]
async fn test_list_projects_only_returns_own() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    // User A creates 2 projects, User B creates 1
    let _pa1 = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let _pa2 = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let _pb1 = common::create_test_project(&app.db_pool, common::USER_B_ID).await;

    let client = reqwest::Client::new();

    // User A lists → sees exactly 2
    let resp = client
        .get(format!("{}/project", &app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 2, "User A should see exactly 2 projects");

    // User B lists → sees exactly 1
    let resp = client
        .get(format!("{}/project", &app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 1, "User B should see exactly 1 project");
}

#[tokio::test]
async fn test_get_project_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // User B tries to GET User A's project → 404
    let resp = client
        .get(format!("{}/project/{}", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B must not read User A's project"
    );
}

#[tokio::test]
async fn test_update_project_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // User B tries to PUT User A's project → 404 (commit 145b8c96: avoid leaking resource existence via IDOR)
    let resp = client
        .put(format!("{}/project/{}", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .header("Content-Type", "application/json")
        .body(r#"{"custom_stack_code":"hijacked","commonDomain":"test.com","dockerhub_user":"x","dockerhub_password":"x","apps":[]}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B must not update User A's project"
    );
}

#[tokio::test]
async fn test_delete_project_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // User B tries to DELETE User A's project → 404 (commit 145b8c96: avoid leaking resource existence via IDOR)
    let resp = client
        .delete(format!("{}/project/{}", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B must not delete User A's project"
    );
}

#[tokio::test]
async fn test_owner_can_access_own_project() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // User A GETs own project → 200
    let resp = client
        .get(format!("{}/project/{}", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "Owner must be able to read own project"
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["item"].is_object(), "expected item object in response");
}

/// A project whose cloud provider is locked to `locked_provider`. Deploying it
/// with a different provider fails with 400 inside the handler, so a 400 proves
/// the request got past the ownership check and a 404 proves it was stopped.
async fn create_project_locked_to(
    pool: &sqlx::PgPool,
    user_id: &str,
    locked_provider: &str,
) -> i32 {
    use sqlx::Row;
    sqlx::query(
        r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, created_at, updated_at)
           VALUES (gen_random_uuid(), $1, 'Locked Provider Project', '{}'::jsonb, $2, NOW(), NOW())
           RETURNING id"#,
    )
    .bind(user_id)
    .bind(serde_json::json!({ "custom": { "locked_cloud_provider": locked_provider } }))
    .fetch_one(pool)
    .await
    .expect("Failed to insert locked project")
    .get::<i32, _>("id")
}

fn deploy_body() -> serde_json::Value {
    serde_json::json!({
        "stack": {
            "vars": [],
            "integrated_features": [],
            "extended_features": [],
            "subscriptions": [],
            "form_app": []
        },
        "cloud": { "provider": "aws", "cloud_token": "test-cloud-token", "save_token": false },
        "server": { "region": "us-east-1", "server": "t3.small", "os": "ubuntu-24.04", "disk_type": "gp3" }
    })
}

#[tokio::test]
async fn test_deploy_rejects_other_users_project() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = create_project_locked_to(&app.db_pool, common::USER_A_ID, "htz").await;

    // User B tries to deploy User A's project → 404
    let resp = reqwest::Client::new()
        .post(format!("{}/project/{}/deploy", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&deploy_body())
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B must not deploy User A's project"
    );
}

#[tokio::test]
async fn test_deploy_with_saved_cloud_rejects_other_users_project() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = create_project_locked_to(&app.db_pool, common::USER_A_ID, "htz").await;
    let cloud_id = common::create_test_cloud(&app.db_pool, common::USER_B_ID, "b-aws", "aws").await;

    // User B deploys User A's project with B's own saved cloud → 404
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/project/{}/deploy/{}",
            &app.address, project_id, cloud_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&deploy_body())
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B must not deploy User A's project"
    );
}

#[tokio::test]
async fn test_deploy_rejects_other_users_saved_cloud() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = create_project_locked_to(&app.db_pool, common::USER_B_ID, "htz").await;
    let cloud_id = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-aws", "aws").await;

    // User B deploys own project with User A's saved cloud → 404
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/project/{}/deploy/{}",
            &app.address, project_id, cloud_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&deploy_body())
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B must not deploy with User A's saved cloud"
    );
}
