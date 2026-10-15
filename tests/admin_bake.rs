//! `POST /api/admin/bake`: everything that can be checked without touching the
//! build box is refused at once, with no job recorded and nothing done to the
//! box. Hetzner is a wiremock; `HETZNER_TOKEN` / `HETZNER_API_BASE_URL` are
//! process-wide, so every case runs inside one test.

mod common;

use chrono::{Duration, Utc};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BOX_IP: &str = "203.0.113.7";
const STACK: &str = "bake-endpoint-stack";

fn admin_jwt() -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let header = URL_SAFE_NO_PAD.encode(json!({"alg": "HS256", "typ": "JWT"}).to_string());
    let payload = URL_SAFE_NO_PAD.encode(
        json!({
            "role": "admin_service",
            "email": "ops@test.com",
            "exp": (Utc::now() + Duration::minutes(30)).timestamp(),
        })
        .to_string(),
    );
    let signature = common::sign_test_jwt(&header, &payload);
    format!("{header}.{payload}.{signature}")
}

fn contract() -> Value {
    json!({ "services": { "app": { "fields": {
        "APP_SECRET": { "mutability": "generated", "type": "hex", "length": 32 }
    } } } })
}

async fn seed_template(pool: &sqlx::PgPool, slug: &str, status: &str, contract: Option<Value>) {
    let id: uuid::Uuid = sqlx::query_scalar(
        r#"INSERT INTO stack_template (creator_user_id, creator_name, name, slug, status, approved_at)
           VALUES ('vendor', 'Vendor', $1, $1, $2, CASE WHEN $2 = 'approved' THEN NOW() END)
           RETURNING id"#,
    )
    .bind(slug)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("seed template");
    sqlx::query(
        r#"INSERT INTO stack_template_version (template_id, version, stack_definition, definition_format, is_latest, config_contract)
           VALUES ($1, '1.2.0', '{}'::jsonb, 'json', true, $2)"#,
    )
    .bind(id)
    .bind(contract)
    .execute(pool)
    .await
    .expect("seed version");
}

/// A deployment whose project has one server at `ip`.
async fn seed_build_box(pool: &sqlx::PgPool, ip: &str) -> i32 {
    let project = common::create_test_project(pool, common::USER_A_ID).await;
    let hash = format!("bake-{}", uuid::Uuid::new_v4());
    let deployment = common::create_test_deployment(pool, common::USER_A_ID, project, &hash).await;
    let server = common::create_test_server(pool, common::USER_A_ID, project, "active", None).await;
    sqlx::query("UPDATE server SET srv_ip = $2 WHERE id = $1")
        .bind(server)
        .bind(ip)
        .execute(pool)
        .await
        .unwrap();
    deployment
}

async fn job_count(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM bake_job")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn post(app: &common::TestApp, body: Value) -> (u16, Value) {
    let resp = reqwest::Client::new()
        .post(format!("{}/api/admin/bake", app.address))
        .bearer_auth(admin_jwt())
        .json(&body)
        .send()
        .await
        .expect("request");
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn a_bake_is_refused_before_the_box_is_touched_and_accepted_when_everything_checks_out() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let hetzner = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/servers"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "servers": [
            { "id": 4242, "name": "build-box", "public_net": { "ipv4": { "ip": BOX_IP } } }
        ] })),
        )
        .mount(&hetzner)
        .await;
    std::env::set_var("HETZNER_API_BASE_URL", hetzner.uri());
    std::env::set_var("HETZNER_TOKEN", "test-hetzner-token");

    seed_template(&app.db_pool, STACK, "approved", Some(contract())).await;
    seed_template(
        &app.db_pool,
        "bake-submitted-stack",
        "submitted",
        Some(contract()),
    )
    .await;
    seed_template(&app.db_pool, "bake-no-contract-stack", "approved", None).await;
    let deployment = seed_build_box(&app.db_pool, BOX_IP).await;
    let unknown_box = seed_build_box(&app.db_pool, "198.51.100.9").await;
    let health = format!("http://{BOX_IP}/health");

    for (body, expected, why) in [
        (
            json!({"deployment_id": 999999, "stack": STACK, "health_url": health}),
            404,
            "unknown deployment",
        ),
        (
            json!({"deployment_id": deployment, "stack": "bake-submitted-stack", "health_url": health}),
            404,
            "template not approved",
        ),
        (
            json!({"deployment_id": deployment, "stack": "bake-no-contract-stack", "health_url": health}),
            409,
            "no contract to sanitize against",
        ),
        (
            json!({"deployment_id": deployment, "stack": STACK, "health_url": "http://198.51.100.1/health"}),
            400,
            "health URL off the box",
        ),
        (
            json!({"deployment_id": unknown_box, "stack": STACK, "health_url": "http://198.51.100.9/health"}),
            409,
            "box Hetzner does not know",
        ),
    ] {
        let (status, resp) = post(&app, body).await;
        assert_eq!(status, expected, "{why}: {resp}");
        assert_eq!(
            job_count(&app.db_pool).await,
            0,
            "{why}: no job may be recorded"
        );
    }

    let (status, resp) = post(
        &app,
        json!({"deployment_id": deployment, "stack": STACK, "health_url": health}),
    )
    .await;
    assert_eq!(status, 202, "{resp}");
    let job = &resp["item"];
    assert_eq!(job["stack"], STACK);
    assert_eq!(
        job["version"], "1.2.0",
        "the latest approved version is baked"
    );
    assert_eq!(job["server_ip"], BOX_IP);

    let (status, resp) = post(
        &app,
        json!({"deployment_id": deployment, "stack": STACK, "health_url": health}),
    )
    .await;
    // Either still running (409) or already failed (no SSH in tests): never two at once.
    if status == 202 {
        assert_eq!(job_count(&app.db_pool).await, 2);
    } else {
        assert_eq!(status, 409, "{resp}");
    }

    let id = job["id"].as_str().unwrap();
    let status = reqwest::Client::new()
        .get(format!("{}/api/admin/bake/{id}", app.address))
        .bearer_auth(admin_jwt())
        .send()
        .await
        .unwrap();
    assert_eq!(status.status().as_u16(), 200);
    let body: Value = status.json().await.unwrap();
    assert_eq!(body["item"]["id"], id);
}

#[tokio::test]
async fn a_user_who_is_not_an_admin_cannot_bake() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let resp = reqwest::Client::new()
        .post(format!("{}/api/admin/bake", app.address))
        .bearer_auth(common::USER_A_TOKEN)
        .json(&json!({"deployment_id": 1, "stack": STACK, "health_url": "http://203.0.113.7/"}))
        .send()
        .await
        .unwrap();
    assert!(
        matches!(resp.status().as_u16(), 401 | 403),
        "got {}",
        resp.status()
    );
}
