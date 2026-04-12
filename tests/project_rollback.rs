mod common;

use reqwest::StatusCode;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

async fn create_test_template_with_requirements(
    pool: &sqlx::PgPool,
    user_id: &str,
    infrastructure_requirements: serde_json::Value,
) -> Uuid {
    let slug = format!("rollback-template-{}", Uuid::new_v4());
    let name = format!("Rollback Template {}", Uuid::new_v4());

    sqlx::query(
        r#"INSERT INTO stack_template (
            creator_user_id,
            name,
            slug,
            status,
            tags,
            tech_stack,
            infrastructure_requirements
        )
        VALUES ($1, $2, $3, 'approved', '[]'::jsonb, '{}'::jsonb, $4)
        RETURNING id"#,
    )
    .bind(user_id)
    .bind(name)
    .bind(slug)
    .bind(infrastructure_requirements)
    .fetch_one(pool)
    .await
    .expect("template insert should succeed")
    .get::<Uuid, _>("id")
}

async fn create_test_template(pool: &sqlx::PgPool, user_id: &str) -> Uuid {
    create_test_template_with_requirements(pool, user_id, json!({})).await
}

async fn create_test_template_version_with_definition(
    pool: &sqlx::PgPool,
    template_id: Uuid,
    version: &str,
    stack_definition: serde_json::Value,
) {
    sqlx::query(
        r#"INSERT INTO stack_template_version (
            template_id,
            version,
            stack_definition,
            definition_format,
            changelog,
            is_latest,
            created_at
        )
        VALUES ($1, $2, $3, 'stacker_project', 'test version', false, NOW())"#,
    )
    .bind(template_id)
    .bind(version)
    .bind(stack_definition)
    .execute(pool)
    .await
    .expect("template version insert should succeed");
}

async fn create_test_template_version(pool: &sqlx::PgPool, template_id: Uuid, version: &str) {
    create_test_template_version_with_definition(pool, template_id, version, json!({})).await
}

async fn mark_project_as_marketplace(
    pool: &sqlx::PgPool,
    project_id: i32,
    template_id: Uuid,
    version: &str,
) {
    sqlx::query(
        r#"UPDATE project
           SET source_template_id = $2,
               template_version = $3
           WHERE id = $1"#,
    )
    .bind(project_id)
    .bind(template_id)
    .bind(version)
    .execute(pool)
    .await
    .expect("project update should succeed");
}

#[tokio::test]
async fn rollback_rejects_non_marketplace_projects() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/project/{}/rollback", app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json().await.expect("error body");
    let message = body["message"].as_str().expect("error message");
    assert!(message.contains("marketplace"));
}

#[tokio::test]
async fn rollback_rejects_unknown_template_versions() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let template_id = create_test_template(&app.db_pool, common::USER_A_ID).await;
    create_test_template_version(&app.db_pool, template_id, "1.0.0").await;
    mark_project_as_marketplace(&app.db_pool, project_id, template_id, "2.0.0").await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/project/{}/rollback", app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "version": "9.9.9" }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json().await.expect("error body");
    let message = body["message"].as_str().expect("error message");
    assert!(message.contains("version"));
    assert!(message.contains("9.9.9"));
}

#[tokio::test]
async fn rollback_rejects_projects_with_multiple_servers() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let template_id = create_test_template(&app.db_pool, common::USER_A_ID).await;
    create_test_template_version(&app.db_pool, template_id, "1.0.0").await;
    mark_project_as_marketplace(&app.db_pool, project_id, template_id, "2.0.0").await;
    common::create_test_server(&app.db_pool, common::USER_A_ID, project_id, "active", None).await;
    common::create_test_server(&app.db_pool, common::USER_A_ID, project_id, "active", None).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/project/{}/rollback", app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json().await.expect("error body");
    let message = body["message"].as_str().expect("error message");
    assert!(message.contains("single server"));
}

#[tokio::test]
async fn rollback_hides_other_users_projects() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let template_id = create_test_template(&app.db_pool, common::USER_A_ID).await;
    create_test_template_version(&app.db_pool, template_id, "1.0.0").await;
    mark_project_as_marketplace(&app.db_pool, project_id, template_id, "2.0.0").await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/project/{}/rollback", app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rollback_does_not_persist_version_change_when_target_validation_fails() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let template_id = create_test_template_with_requirements(
        &app.db_pool,
        common::USER_A_ID,
        json!({ "supported_clouds": ["aws"] }),
    )
    .await;
    create_test_template_version_with_definition(
        &app.db_pool,
        template_id,
        "1.0.0",
        json!({
            "custom": {
                "web": [],
                "custom_stack_code": "rolled-back-stack"
            }
        }),
    )
    .await;
    mark_project_as_marketplace(&app.db_pool, project_id, template_id, "2.0.0").await;
    common::create_test_server(&app.db_pool, common::USER_A_ID, project_id, "active", None).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/project/{}/rollback", app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json().await.expect("error body");
    let message = body["message"].as_str().expect("error message");
    assert!(message.contains("not supported"));

    let project_row = sqlx::query(
        r#"SELECT name, template_version
           FROM project
           WHERE id = $1"#,
    )
    .bind(project_id)
    .fetch_one(&app.db_pool)
    .await
    .expect("project fetch should succeed");

    assert_eq!(project_row.get::<String, _>("name"), "Test Project");
    assert_eq!(
        project_row.get::<Option<String>, _>("template_version"),
        Some("2.0.0".to_string())
    );
}
