/// Security tests: sensitive fields in stack_definition must be redacted
/// for any caller of the public template detail endpoints.
///
/// Covers:
///   GET /api/templates/{slug}
///   GET /api/v1/templates/{slug}
mod common;

use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::Row;

async fn insert_approved_template(pool: &sqlx::PgPool, slug: &str) -> uuid::Uuid {
    sqlx::query(
        r#"INSERT INTO stack_template (
            creator_user_id, name, slug, status,
            tags, tech_stack, infrastructure_requirements,
            approved_at
        )
        VALUES ($1, $2, $3, 'approved', '[]'::jsonb, '{}'::jsonb, '{}'::jsonb, NOW())
        RETURNING id"#,
    )
    .bind(common::USER_A_ID)
    .bind(format!("Sensitive Test Template {}", slug))
    .bind(slug)
    .fetch_one(pool)
    .await
    .expect("insert template")
    .get::<uuid::Uuid, _>("id")
}

/// Seed a template whose stack_definition is a JSON object (definition_format = 'json')
/// with sensitive fields in the {key, value} pair format used by ProjectForm.
async fn seed_template_with_sensitive_json_definition(
    pool: &sqlx::PgPool,
    slug: &str,
) -> uuid::Uuid {
    let template_id = insert_approved_template(pool, slug).await;

    let stack_definition = json!({
        "custom": {
            "custom_stack_code": "myapp",
            "web": []
        },
        "deploy": {
            "stack": {
                "vars": [
                    { "key": "DB_HOST",     "value": "db.internal" },
                    { "key": "DB_PASSWORD", "value": "super_secret_pw" },
                    { "key": "DB_USER",     "value": "appuser" },
                    { "key": "api_key",     "value": "sk-live-abc123" },
                    { "key": "AUTH_KEY",    "value": "auth-token-xyz" },
                    { "key": "APP_PORT",    "value": "3000" }
                ]
            },
            "nested": {
                "credentials": {
                    "passwd": "root_pass",
                    "username": "root"
                }
            }
        }
    });

    sqlx::query(
        r#"INSERT INTO stack_template_version (
            template_id, version, stack_definition,
            definition_format, is_latest
        )
        VALUES ($1, '1.0.0', $2, 'json', true)"#,
    )
    .bind(template_id)
    .bind(stack_definition)
    .execute(pool)
    .await
    .expect("insert template version");

    template_id
}

/// Seed a template whose stack_definition is a Docker Compose YAML string
/// (definition_format = 'yaml') — the format used by real marketplace templates.
async fn seed_template_with_sensitive_yaml_definition(
    pool: &sqlx::PgPool,
    slug: &str,
) -> uuid::Uuid {
    let template_id = insert_approved_template(pool, slug).await;

    // Mirrors the actual format seen at /api/templates/myproject15
    let yaml = r#"version: '3.8'
services:
  app:
    image: flowiseai/flowise:latest
    environment:
      PORT: '3000'
      APP_HOST: localhost
      FLOWISE_USERNAME: admin
      FLOWISE_PASSWORD: admin123
      DATABASE_PASSWORD: Qwerty123
      SECRETKEY: change-me-please
    restart: always
"#;

    sqlx::query(
        r#"INSERT INTO stack_template_version (
            template_id, version, stack_definition,
            definition_format, is_latest
        )
        VALUES ($1, '1.0.0', $2::text::jsonb, 'yaml', true)"#,
    )
    .bind(template_id)
    .bind(yaml)
    .execute(pool)
    .await
    .expect("insert yaml template version");

    template_id
}

// ────────────────────────────────────────────────────────────────────
// /api/templates/{slug}
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn anonymous_user_cannot_see_password_values_in_template_detail() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let slug = "redact-test-anon-v1";
    seed_template_with_sensitive_json_definition(&app.db_pool, slug).await;

    let response = reqwest::Client::new()
        .get(format!("{}/api/templates/{}", app.address, slug))
        .send()
        .await
        .expect("detail request");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response.json().await.expect("json body");
    let sd = &body["item"]["latest_version"]["stack_definition"];

    // Sensitive values must be redacted
    let vars = sd["deploy"]["stack"]["vars"]
        .as_array()
        .expect("vars array");

    let find = |key: &str| {
        vars.iter()
            .find(|v| v["key"] == key)
            .map(|v| v["value"].as_str().unwrap_or("").to_string())
    };

    assert_eq!(
        Some("***REDACTED***".to_string()),
        find("DB_PASSWORD"),
        "DB_PASSWORD value must be redacted"
    );
    assert_eq!(
        Some("***REDACTED***".to_string()),
        find("api_key"),
        "api_key value must be redacted"
    );
    assert_eq!(
        Some("***REDACTED***".to_string()),
        find("AUTH_KEY"),
        "AUTH_KEY value must be redacted"
    );

    // Non-sensitive values must be preserved
    assert_eq!(
        Some("db.internal".to_string()),
        find("DB_HOST"),
        "DB_HOST value must not be redacted"
    );
    assert_eq!(
        Some("appuser".to_string()),
        find("DB_USER"),
        "DB_USER value must not be redacted"
    );
    assert_eq!(
        Some("3000".to_string()),
        find("APP_PORT"),
        "APP_PORT value must not be redacted"
    );

    // Nested sensitive field must also be redacted
    assert_eq!(
        "***REDACTED***",
        sd["deploy"]["nested"]["credentials"]["passwd"],
        "nested passwd must be redacted"
    );
    // Non-sensitive sibling must survive
    assert_eq!(
        "root",
        sd["deploy"]["nested"]["credentials"]["username"],
        "nested username must not be redacted"
    );
}

#[tokio::test]
async fn authenticated_user_also_receives_redacted_stack_definition() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let slug = "redact-test-auth-v1";
    seed_template_with_sensitive_json_definition(&app.db_pool, slug).await;

    // Authenticated request
    let response = reqwest::Client::new()
        .get(format!("{}/api/templates/{}", app.address, slug))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("detail request");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response.json().await.expect("json body");
    let sd = &body["item"]["latest_version"]["stack_definition"];

    let vars = sd["deploy"]["stack"]["vars"]
        .as_array()
        .expect("vars array");

    let find = |key: &str| {
        vars.iter()
            .find(|v| v["key"] == key)
            .map(|v| v["value"].as_str().unwrap_or("").to_string())
    };

    assert_eq!(
        Some("***REDACTED***".to_string()),
        find("DB_PASSWORD"),
        "DB_PASSWORD must be redacted for authenticated users too"
    );
    assert_eq!(
        Some("db.internal".to_string()),
        find("DB_HOST"),
        "DB_HOST must remain visible"
    );
}

// ────────────────────────────────────────────────────────────────────
// /api/v1/templates/{slug}
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn v1_template_detail_also_redacts_sensitive_fields() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let slug = "redact-test-v1-path";
    seed_template_with_sensitive_json_definition(&app.db_pool, slug).await;

    let response = reqwest::Client::new()
        .get(format!("{}/api/v1/templates/{}", app.address, slug))
        .send()
        .await
        .expect("v1 detail request");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response.json().await.expect("json body");
    let sd = &body["item"]["latest_version"]["stack_definition"];

    let vars = sd["deploy"]["stack"]["vars"]
        .as_array()
        .expect("vars array");

    let password_var = vars
        .iter()
        .find(|v| v["key"] == "DB_PASSWORD")
        .expect("DB_PASSWORD var");

    assert_eq!(
        "***REDACTED***", password_var["value"],
        "/api/v1/templates/{slug} must redact DB_PASSWORD"
    );
}

// ────────────────────────────────────────────────────────────────────
// YAML format templates (definition_format = 'yaml')
// This is the format used by real marketplace templates like myproject15
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn yaml_stack_definition_has_passwords_redacted() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let slug = "redact-test-yaml-format";
    seed_template_with_sensitive_yaml_definition(&app.db_pool, slug).await;

    let response = reqwest::Client::new()
        .get(format!("{}/api/templates/{}", app.address, slug))
        .send()
        .await
        .expect("detail request");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response.json().await.expect("json body");
    let sd = body["item"]["latest_version"]["stack_definition"]
        .as_str()
        .expect("stack_definition must be a YAML string");

    assert!(
        !sd.contains("admin123"),
        "FLOWISE_PASSWORD plaintext must not appear: {}",
        sd
    );
    assert!(
        !sd.contains("Qwerty123"),
        "DATABASE_PASSWORD plaintext must not appear: {}",
        sd
    );
    assert!(
        !sd.contains("change-me-please"),
        "SECRETKEY plaintext must not appear: {}",
        sd
    );
    assert!(
        sd.contains("***REDACTED***"),
        "redacted marker must be present: {}",
        sd
    );
    // Non-sensitive values must survive
    assert!(
        sd.contains("localhost") || sd.contains("APP_HOST"),
        "APP_HOST must survive: {}",
        sd
    );
    assert!(
        sd.contains("admin") || sd.contains("FLOWISE_USERNAME"),
        "FLOWISE_USERNAME (non-sensitive) must survive: {}",
        sd
    );
}
