mod common;

use reqwest::StatusCode;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

fn project_payload(stack_code: &str) -> serde_json::Value {
    json!({
        "custom": {
            "web": [],
            "custom_stack_code": stack_code
        }
    })
}

async fn insert_template_with_version(
    pool: &sqlx::PgPool,
    user_id: &str,
    version: &str,
    stack_definition: serde_json::Value,
) -> Uuid {
    let template_id = sqlx::query(
        r#"INSERT INTO stack_template (
            creator_user_id,
            name,
            slug,
            status,
            tags,
            tech_stack,
            infrastructure_requirements
        )
        VALUES ($1, $2, $3, 'approved', '[]'::jsonb, '{}'::jsonb, '{}'::jsonb)
        RETURNING id"#,
    )
    .bind(user_id)
    .bind(format!("Template {}", Uuid::new_v4()))
    .bind(format!("rollback-template-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("Failed to insert template")
    .get::<Uuid, _>("id");

    sqlx::query(
        r#"INSERT INTO stack_template_version (
            template_id,
            version,
            stack_definition,
            definition_format,
            changelog,
            is_latest
        )
        VALUES ($1, $2, $3, 'json', NULL, true)"#,
    )
    .bind(template_id)
    .bind(version)
    .bind(stack_definition)
    .execute(pool)
    .await
    .expect("Failed to insert template version");

    template_id
}

async fn insert_project_with_template(
    pool: &sqlx::PgPool,
    user_id: &str,
    template_id: Uuid,
    version: &str,
    payload: serde_json::Value,
) -> i32 {
    sqlx::query(
        r#"INSERT INTO project (
            stack_id,
            user_id,
            name,
            metadata,
            request_json,
            source_template_id,
            template_version,
            created_at,
            updated_at
        )
        VALUES (gen_random_uuid(), $1, 'user-project-name', $2, $2, $3, $4, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user_id)
    .bind(payload)
    .bind(template_id)
    .bind(version)
    .fetch_one(pool)
    .await
    .expect("Failed to insert project")
    .get::<i32, _>("id")
}

#[tokio::test]
async fn rollback_rejects_non_marketplace_projects() {
    let Some(app) = common::spawn_app().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;

    let response = reqwest::Client::new()
        .post(format!("{}/project/{}/rollback", app.address, project_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .expect("rollback request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rollback_rejects_unknown_versions() {
    let Some(app) = common::spawn_app().await else {
        return;
    };

    let template_id = insert_template_with_version(
        &app.db_pool,
        common::USER_A_ID,
        "1.0.0",
        project_payload("template-stack"),
    )
    .await;
    let project_id = insert_project_with_template(
        &app.db_pool,
        common::USER_A_ID,
        template_id,
        "1.0.0",
        project_payload("template-stack"),
    )
    .await;

    let response = reqwest::Client::new()
        .post(format!("{}/project/{}/rollback", app.address, project_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({ "version": "9.9.9" }))
        .send()
        .await
        .expect("rollback request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rollback_rejects_projects_with_multiple_servers() {
    let Some(app) = common::spawn_app().await else {
        return;
    };

    let template_id = insert_template_with_version(
        &app.db_pool,
        common::USER_A_ID,
        "1.0.0",
        project_payload("template-stack"),
    )
    .await;
    let project_id = insert_project_with_template(
        &app.db_pool,
        common::USER_A_ID,
        template_id,
        "1.0.0",
        project_payload("template-stack"),
    )
    .await;

    let cloud_id = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "saved-aws", "aws").await;
    let server_a = common::create_test_server(&app.db_pool, common::USER_A_ID, project_id, "active", Some("secret/users/test_user_id/ssh_keys/1")).await;
    let server_b = common::create_test_server(&app.db_pool, common::USER_A_ID, project_id, "active", Some("secret/users/test_user_id/ssh_keys/2")).await;

    sqlx::query("UPDATE server SET cloud_id = $1 WHERE id = ANY($2)")
        .bind(cloud_id)
        .bind(vec![server_a, server_b])
        .execute(&app.db_pool)
        .await
        .expect("Failed to attach cloud to servers");

    let response = reqwest::Client::new()
        .post(format!("{}/project/{}/rollback", app.address, project_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .expect("rollback request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rollback_hides_other_users_projects() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let template_id = insert_template_with_version(
        &app.db_pool,
        common::USER_A_ID,
        "1.0.0",
        project_payload("template-stack"),
    )
    .await;
    let project_id = insert_project_with_template(
        &app.db_pool,
        common::USER_A_ID,
        template_id,
        "1.0.0",
        project_payload("template-stack"),
    )
    .await;

    let response = reqwest::Client::new()
        .post(format!("{}/project/{}/rollback", app.address, project_id))
        .bearer_auth(common::USER_B_TOKEN)
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .expect("rollback request failed");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rollback_succeeds_for_single_server_marketplace_project() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let current_payload = project_payload("current-template");
    let target_payload = project_payload("target-template");
    let template_id = insert_template_with_version(&app.db_pool, common::USER_A_ID, "1.0.0", current_payload.clone()).await;
    sqlx::query(
        r#"INSERT INTO stack_template_version (
            template_id,
            version,
            stack_definition,
            definition_format,
            changelog,
            is_latest
        )
        VALUES ($1, '0.9.0', $2, 'json', NULL, false)"#,
    )
    .bind(template_id)
    .bind(target_payload.clone())
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert rollback target version");

    let project_id = insert_project_with_template(
        &app.db_pool,
        common::USER_A_ID,
        template_id,
        "1.0.0",
        current_payload,
    )
    .await;
    let cloud_id = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "saved-aws", "aws").await;
    let server_id = common::create_test_server(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        "active",
        Some("secret/users/test_user_id/ssh_keys/1"),
    )
    .await;

    sqlx::query(
        r#"UPDATE server
           SET cloud_id = $1,
               region = 'us-east-1',
               server = 't3.small',
               os = 'ubuntu-24.04',
               disk_type = 'gp3'
           WHERE id = $2"#,
    )
    .bind(cloud_id)
    .bind(server_id)
    .execute(&app.db_pool)
    .await
    .expect("Failed to prepare server");

    Mock::given(method("GET"))
        .and(path(format!("/v1/secret/users/{}/ssh_keys/{}", common::USER_A_ID, server_id)))
        .and(header("X-Vault-Token", "test-vault-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "public_key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITestkey",
                "private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\nkey\n-----END OPENSSH PRIVATE KEY-----"
            }
        })))
        .mount(&app.vault_server)
        .await;

    let response = reqwest::Client::new()
        .post(format!("{}/project/{}/rollback", app.address, project_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({ "version": "0.9.0" }))
        .send()
        .await
        .expect("rollback request failed");

    assert_eq!(response.status(), StatusCode::OK);

    let project_row = sqlx::query("SELECT name, template_version, metadata FROM project WHERE id = $1")
        .bind(project_id)
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch updated project");
    assert_eq!(project_row.get::<String, _>("name"), "user-project-name");
    assert_eq!(project_row.get::<Option<String>, _>("template_version").as_deref(), Some("0.9.0"));
}
