mod common;

use reqwest::StatusCode;
use serde_json::json;
use sqlx::Row;

async fn seed_project_and_deployment(pool: &sqlx::PgPool, user_id: &str) -> (i32, i32, String) {
    let project_id = common::create_test_project(pool, user_id).await;
    let hash = format!("team-dpl-{}", uuid::Uuid::new_v4());
    let deployment_id = common::create_test_deployment(pool, user_id, project_id, &hash).await;
    (project_id, deployment_id, hash)
}

async fn share_project(
    app_address: &str,
    project_id: i32,
    actor_token: &str,
    member_user_id: &str,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{}/project/{}/members", app_address, project_id))
        .header("Authorization", format!("Bearer {}", actor_token))
        .json(&json!({
            "user_id": member_user_id,
            "role": "viewer"
        }))
        .send()
        .await
        .expect("share request failed")
}

async fn list_shared_projects(app_address: &str, token: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("{}/project/shared", app_address))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("shared project list request failed")
}

async fn list_project_members(
    app_address: &str,
    project_id: i32,
    token: &str,
) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("{}/project/{}/members", app_address, project_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("project members list request failed")
}

async fn remove_project_member(
    app_address: &str,
    project_id: i32,
    actor_token: &str,
    member_user_id: &str,
) -> reqwest::Response {
    reqwest::Client::new()
        .delete(format!(
            "{}/project/{}/members/{}",
            app_address, project_id, member_user_id
        ))
        .header("Authorization", format!("Bearer {}", actor_token))
        .send()
        .await
        .expect("project member delete request failed")
}

async fn create_test_project_with_payload(
    pool: &sqlx::PgPool,
    user_id: &str,
    name: &str,
    metadata: serde_json::Value,
    request_json: serde_json::Value,
) -> i32 {
    sqlx::query(
        r#"INSERT INTO project (stack_id, user_id, name, metadata, request_json, created_at, updated_at)
        VALUES (gen_random_uuid(), $1, $2, $3, $4, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user_id)
    .bind(name)
    .bind(metadata)
    .bind(request_json)
    .fetch_one(pool)
    .await
    .map(|row| row.get::<i32, _>("id"))
    .expect("Failed to insert test project with payload")
}

#[tokio::test]
async fn owner_can_share_project_with_viewer_by_user_id() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;

    let response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json response");
    assert_eq!(body["item"]["user_id"].as_str(), Some(common::USER_B_ID));
    assert_eq!(body["item"]["role"].as_str(), Some("viewer"));
}

#[tokio::test]
async fn non_owner_cannot_share_project() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;

    let response = share_project(
        &app.address,
        project_id,
        common::USER_B_TOKEN,
        common::USER_A_ID,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn viewer_can_get_shared_project_latest_deployment() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let (project_id, _deployment_id, deployment_hash) =
        seed_project_and_deployment(&app.db_pool, common::USER_A_ID).await;
    let share_response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/deployments/project/{}",
            app.address, project_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("deployment-by-project request failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json response");
    assert_eq!(
        body["item"]["deployment_hash"].as_str(),
        Some(deployment_hash.as_str())
    );
}

#[tokio::test]
async fn viewer_can_list_shared_project_deployments_with_project_filter() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let (project_id, _deployment_id, deployment_hash) =
        seed_project_and_deployment(&app.db_pool, common::USER_A_ID).await;
    let share_response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/deployments?project_id={}",
            app.address, project_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("deployment list request failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json response");
    let list = body["list"].as_array().expect("list should be array");
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0]["deployment_hash"].as_str(),
        Some(deployment_hash.as_str())
    );
}

#[tokio::test]
async fn viewer_cannot_get_raw_project_payload() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let share_response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let response = reqwest::Client::new()
        .get(format!("{}/project/{}", app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("project get request failed");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn viewer_can_list_projects_shared_with_them() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let share_response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let response = list_shared_projects(&app.address, common::USER_B_TOKEN).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json response");
    let list = body["list"].as_array().expect("list should be array");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"].as_i64(), Some(i64::from(project_id)));
    assert_eq!(list[0]["name"].as_str(), Some("Test Project"));
    assert_eq!(list[0]["role"].as_str(), Some("viewer"));
    assert!(list[0]["shared_at"].as_str().is_some());
}

#[tokio::test]
async fn shared_projects_list_excludes_sensitive_fields() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = create_test_project_with_payload(
        &app.db_pool,
        common::USER_A_ID,
        "Sensitive Project",
        json!({"secret_metadata": "should-not-leak"}),
        json!({"secret_request": "also-hidden"}),
    )
    .await;
    let share_response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let response = list_shared_projects(&app.address, common::USER_B_TOKEN).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json response");
    let list = body["list"].as_array().expect("list should be array");
    assert_eq!(list.len(), 1);
    let project = list[0].as_object().expect("project should be an object");
    assert!(!project.contains_key("metadata"));
    assert!(!project.contains_key("request_json"));
}

#[tokio::test]
async fn shared_projects_list_only_returns_projects_shared_to_requesting_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let visible_project_id = create_test_project_with_payload(
        &app.db_pool,
        common::USER_A_ID,
        "Visible Project",
        json!({}),
        json!({}),
    )
    .await;
    let hidden_project_id = create_test_project_with_payload(
        &app.db_pool,
        common::USER_A_ID,
        "Hidden Project",
        json!({}),
        json!({}),
    )
    .await;

    let share_response = share_project(
        &app.address,
        visible_project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let response = list_shared_projects(&app.address, common::USER_B_TOKEN).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json response");
    let list = body["list"].as_array().expect("list should be array");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"].as_i64(), Some(i64::from(visible_project_id)));
    assert_ne!(list[0]["id"].as_i64(), Some(i64::from(hidden_project_id)));
}

#[tokio::test]
async fn shared_project_does_not_change_existing_project_list_contract() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let share_response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let response = reqwest::Client::new()
        .get(format!("{}/project", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("project list request failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json response");
    let list = body["list"].as_array().expect("list should be array");
    assert!(list.is_empty());
}

#[tokio::test]
async fn owner_can_list_project_members() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let share_response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let response = list_project_members(&app.address, project_id, common::USER_A_TOKEN).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json response");
    let list = body["list"].as_array().expect("list should be array");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["user_id"].as_str(), Some(common::USER_B_ID));
    assert_eq!(list[0]["role"].as_str(), Some("viewer"));
}

#[tokio::test]
async fn non_owner_cannot_list_project_members() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let share_response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let response = list_project_members(&app.address, project_id, common::USER_B_TOKEN).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn owner_can_remove_project_member() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let share_response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let response = remove_project_member(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let list_response = list_project_members(&app.address, project_id, common::USER_A_TOKEN).await;
    assert_eq!(list_response.status(), StatusCode::OK);
    let body: serde_json::Value = list_response.json().await.expect("json response");
    let list = body["list"].as_array().expect("list should be array");
    assert!(list.is_empty());
}

#[tokio::test]
async fn removed_viewer_loses_shared_project_access() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let (project_id, _deployment_id, _deployment_hash) =
        seed_project_and_deployment(&app.db_pool, common::USER_A_ID).await;
    let share_response = share_project(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(share_response.status(), StatusCode::OK);

    let remove_response = remove_project_member(
        &app.address,
        project_id,
        common::USER_A_TOKEN,
        common::USER_B_ID,
    )
    .await;
    assert_eq!(remove_response.status(), StatusCode::NO_CONTENT);

    let shared_projects_response = list_shared_projects(&app.address, common::USER_B_TOKEN).await;
    assert_eq!(shared_projects_response.status(), StatusCode::OK);
    let shared_projects_body: serde_json::Value = shared_projects_response
        .json()
        .await
        .expect("json response");
    let shared_projects = shared_projects_body["list"]
        .as_array()
        .expect("list should be array");
    assert!(shared_projects.is_empty());

    let deployment_response = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/deployments/project/{}",
            app.address, project_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("deployment-by-project request failed");

    assert_eq!(deployment_response.status(), StatusCode::NOT_FOUND);
}
