//! `POST /api/admin/bake`: everything that can be checked without touching the
//! build box is refused at once, with no job recorded and nothing done to the
//! box. Hetzner is a wiremock; `HETZNER_TOKEN` / `HETZNER_API_BASE_URL` are
//! process-wide, so every case runs inside one test.
//!
//! A bake takes only the caller's own build box: sanitizing wipes the box, so
//! a mistyped `deployment_id` must never reach a buyer's server.

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

/// An admin signed in through the User Service: User A with the
/// `group_admin` role, the way the bake harness calls with its CLI token.
async fn spawn_admin_app() -> Option<common::TestApp> {
    let auth = MockServer::start().await;
    let user = stacker::forms::user::User {
        id: common::USER_A_ID.to_string(),
        email: common::USER_A_EMAIL.to_string(),
        role: "group_admin".to_string(),
        email_confirmed: true,
        ..Default::default()
    };
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(stacker::forms::user::UserForm { user }),
        )
        .mount(&auth)
        .await;
    let mut configuration =
        stacker::configuration::get_configuration().expect("Failed to get configuration");
    configuration.auth_url = format!("{}/me", auth.uri());
    // The mock must outlive the app, which lives for the rest of the test.
    std::mem::forget(auth);
    common::spawn_app_with_configuration(configuration).await
}

/// A deployment whose project has one server at `ip`.
async fn seed_build_box(pool: &sqlx::PgPool, ip: &str) -> i32 {
    seed_build_box_of(pool, ip, common::USER_A_ID).await
}

/// A deployment of `owner` whose project has one server at `ip`.
async fn seed_build_box_of(pool: &sqlx::PgPool, ip: &str, owner: &str) -> i32 {
    let project = common::create_test_project(pool, owner).await;
    let hash = format!("bake-{}", uuid::Uuid::new_v4());
    let deployment = common::create_test_deployment(pool, owner, project, &hash).await;
    let server = common::create_test_server(pool, owner, project, "active", None).await;
    sqlx::query("UPDATE server SET srv_ip = $2 WHERE id = $1")
        .bind(server)
        .bind(ip)
        .execute(pool)
        .await
        .unwrap();
    deployment
}

/// Another server record in the deployment's project, at `ip`; returns its id.
async fn add_server(pool: &sqlx::PgPool, deployment: i32, ip: &str) -> i32 {
    let project: i32 = sqlx::query_scalar("SELECT project_id FROM deployment WHERE id = $1")
        .bind(deployment)
        .fetch_one(pool)
        .await
        .unwrap();
    let server = common::create_test_server(pool, common::USER_A_ID, project, "active", None).await;
    sqlx::query("UPDATE server SET srv_ip = $2 WHERE id = $1")
        .bind(server)
        .bind(ip)
        .execute(pool)
        .await
        .unwrap();
    server
}

async fn job_count(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM bake_job")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn post(app: &common::TestApp, body: Value) -> (u16, Value) {
    post_as(app, "admin-user-token", body).await
}

async fn post_as(app: &common::TestApp, token: &str, body: Value) -> (u16, Value) {
    let resp = reqwest::Client::new()
        .post(format!("{}/api/admin/bake", app.address))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .expect("request");
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn a_bake_is_refused_before_the_box_is_touched_and_accepted_when_everything_checks_out() {
    let Some(app) = spawn_admin_app().await else {
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
    // Someone else's server at a known IP, and a buyer's Quick Deploy server.
    let other_users_box = seed_build_box_of(&app.db_pool, BOX_IP, common::USER_B_ID).await;
    let quick_deploy_box = seed_build_box(&app.db_pool, BOX_IP).await;
    sqlx::query(
        r#"INSERT INTO managed_usage (deployment_hash, user_id, stack, server_type, daily_rate_minor, monthly_cap_minor)
           SELECT deployment_hash, user_id, $2, 'cpx22', 100, 3000 FROM deployment WHERE id = $1"#,
    )
    .bind(quick_deploy_box)
    .bind(STACK)
    .execute(&app.db_pool)
    .await
    .expect("seed managed usage");

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
        (
            json!({"deployment_id": other_users_box, "stack": STACK, "health_url": health}),
            403,
            "another user's server",
        ),
        (
            json!({"deployment_id": quick_deploy_box, "stack": STACK, "health_url": health}),
            409,
            "a Quick Deploy server is a buyer's, not a build box",
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

    // A service token owns no build box, so it cannot bake one.
    let (status, resp) = post_as(
        &app,
        &admin_jwt(),
        json!({"deployment_id": deployment, "stack": STACK, "health_url": health}),
    )
    .await;
    assert_eq!(status, 403, "admin_service: {resp}");
    assert_eq!(job_count(&app.db_pool).await, 0, "admin_service: no job");

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
        .bearer_auth("admin-user-token")
        .send()
        .await
        .unwrap();
    assert_eq!(status.status().as_u16(), 200);
    let body: Value = status.json().await.unwrap();
    assert_eq!(body["item"]["id"], id);

    // A project redeployed with --force-new keeps a server record per attempt,
    // and Hetzner hands the new box an old box's IP. `server_id` - the Hetzner
    // id, as the `bake` binary takes it - names the build box among them.
    let redeployed = seed_build_box(&app.db_pool, "198.51.100.40").await;
    add_server(&app.db_pool, redeployed, BOX_IP).await;
    let build_box = add_server(&app.db_pool, redeployed, BOX_IP).await;
    let jobs_before = job_count(&app.db_pool).await;
    for (server_id, why) in [
        (Value::Null, "several servers and no server_id"),
        (
            json!(9999),
            "a server_id that is none of the project's boxes",
        ),
    ] {
        let (status, resp) = post(
            &app,
            json!({"deployment_id": redeployed, "stack": STACK, "health_url": health, "server_id": server_id}),
        )
        .await;
        assert_eq!(status, 409, "{why}: {resp}");
        assert_eq!(job_count(&app.db_pool).await, jobs_before, "{why}: no job");
    }
    let (status, resp) = post(
        &app,
        json!({"deployment_id": redeployed, "stack": STACK, "health_url": health, "server_id": 4242}),
    )
    .await;
    assert_eq!(status, 202, "{resp}");
    assert_eq!(resp["item"]["server_ip"], BOX_IP);
    assert_eq!(
        resp["item"]["server_id"], build_box,
        "the newest record at the box's IP: older ones belong to removed boxes"
    );
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
