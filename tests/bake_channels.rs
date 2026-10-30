//! Release channels for baked snapshots (BAKE_PIPELINE_PLAN.md, Stage 4).
//!
//! A bake started from the API lands on `unstable`. Buyers get a snapshot only
//! once a green smoke clone promotes it to `latest`; the old `latest` becomes
//! `stable`. Snapshots recorded before channels existed (and by the `bake`
//! binary) keep the old "newest healthy" behaviour, so live links do not break.

mod common;

use chrono::{Duration, Utc};
use serde_json::{json, Value};
use stacker::db::{bake_job, bake_registry, baked_snapshot};

const PROVIDER: &str = "hetzner";

/// A snapshot recorded the old way: served at once.
async fn legacy(pool: &sqlx::PgPool, stack: &str, version: &str, image_id: i64) -> i32 {
    baked_snapshot::record(
        pool, stack, version, PROVIDER, image_id, true, None, None, None, None,
    )
    .await
    .expect("record legacy snapshot")
    .id
}

/// A snapshot baked through the API: waits on `unstable`.
async fn baked(pool: &sqlx::PgPool, stack: &str, version: &str, image_id: i64) -> i32 {
    baked_snapshot::record_unstable(
        pool, stack, version, PROVIDER, image_id, true, None, None, None, None,
    )
    .await
    .expect("record unstable snapshot")
    .id
}

async fn served(pool: &sqlx::PgPool, stack: &str) -> Option<i64> {
    baked_snapshot::resolve_latest(pool, stack, PROVIDER)
        .await
        .expect("resolve latest")
        .map(|s| s.image_id)
}

async fn served_version(pool: &sqlx::PgPool, stack: &str, version: &str) -> Option<i64> {
    baked_snapshot::resolve(pool, stack, version, PROVIDER)
        .await
        .expect("resolve version")
        .map(|s| s.image_id)
}

#[tokio::test]
async fn a_bake_reaches_buyers_only_after_promotion() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let pool = &app.db_pool;
    let stack = "chan-promote";

    legacy(pool, stack, "1.0.0", 100).await;
    let b = baked(pool, stack, "1.1.0", 200).await;

    assert_eq!(
        served(pool, stack).await,
        Some(100),
        "the unpromoted bake is not served"
    );
    assert_eq!(
        served_version(pool, stack, "1.1.0").await,
        None,
        "not even when its version is asked for"
    );
    let state = bake_registry::state(pool, stack, PROVIDER).await.unwrap();
    assert_eq!(state.unstable, Some(b));
    assert_eq!(state.latest, None);

    let moves = bake_registry::promote(pool, b).await.expect("promote");
    assert!(moves.is_some(), "first promotion moves latest");
    assert_eq!(served(pool, stack).await, Some(200));
    assert_eq!(served_version(pool, stack, "1.1.0").await, Some(200));

    // Promoting again is a no-op, not an error: the harness may retry.
    assert_eq!(bake_registry::promote(pool, b).await.expect("repeat"), None);

    // The next bake waits too; once promoted, the old latest becomes stable.
    let c = baked(pool, stack, "1.2.0", 300).await;
    assert_eq!(served(pool, stack).await, Some(200));
    bake_registry::promote(pool, c).await.expect("promote c");
    let state = bake_registry::state(pool, stack, PROVIDER).await.unwrap();
    assert_eq!((state.latest, state.stable), (Some(c), Some(b)));
    assert_eq!(served(pool, stack).await, Some(300));
    assert_eq!(
        served_version(pool, stack, "1.1.0").await,
        Some(200),
        "stable stays deployable by its version"
    );
}

#[tokio::test]
async fn a_bake_that_was_never_promoted_stays_hidden_after_the_next_one() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let pool = &app.db_pool;
    let stack = "chan-failed-smoke";

    let failed = baked(pool, stack, "1.0.0", 100).await;
    let next = baked(pool, stack, "1.0.0", 200).await;

    assert_eq!(
        served(pool, stack).await,
        None,
        "losing the unstable pointer must not make the failed bake servable"
    );
    assert!(
        bake_registry::promote(pool, failed).await.is_err(),
        "only the current unstable bake can be promoted"
    );
    bake_registry::promote(pool, next)
        .await
        .expect("promote next");
    assert_eq!(served(pool, stack).await, Some(200));
}

#[tokio::test]
async fn a_legacy_snapshot_cannot_be_promoted() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let pool = &app.db_pool;
    let id = legacy(pool, "chan-legacy", "1.0.0", 100).await;
    assert!(bake_registry::promote(pool, id).await.is_err());
    assert!(
        bake_registry::promote(pool, 999_999).await.is_err(),
        "unknown id"
    );
    assert_eq!(served(pool, "chan-legacy").await, Some(100));
}

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

async fn promote_job(app: &common::TestApp, id: &str, token: &str) -> (u16, Value) {
    let resp = reqwest::Client::new()
        .post(format!("{}/api/admin/bake/{id}/promote", app.address))
        .bearer_auth(token)
        .send()
        .await
        .expect("request");
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn the_harness_promotes_a_finished_bake_by_its_job_id() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let pool = &app.db_pool;
    let stack = "chan-endpoint";
    let job = bake_job::insert(
        pool,
        bake_job::NewBakeJob {
            stack,
            version: "1.0.0",
            deployment_id: 1,
            server_id: 1,
            server_ip: "203.0.113.9",
            created_by: "ops@test.com",
        },
    )
    .await
    .expect("job");
    let id = job.id.to_string();

    let (status, body) = promote_job(&app, &id, &admin_jwt()).await;
    assert_eq!(status, 409, "a running bake has no snapshot yet: {body}");

    let snapshot = baked(pool, stack, "1.0.0", 100).await;
    bake_job::mark_done(pool, job.id, 100, snapshot)
        .await
        .expect("done");

    let (status, body) = promote_job(&app, &id, common::USER_A_TOKEN).await;
    assert!(matches!(status, 401 | 403), "not an admin: {status} {body}");
    assert_eq!(served(pool, stack).await, None);

    let (status, body) = promote_job(&app, &id, &admin_jwt()).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["item"]["latest"], snapshot, "{body}");
    assert_eq!(served(pool, stack).await, Some(100));

    let (status, body) = promote_job(&app, &id, &admin_jwt()).await;
    assert_eq!(status, 200, "a repeated promote is a no-op: {body}");

    let (status, _) = promote_job(&app, &uuid::Uuid::new_v4().to_string(), &admin_jwt()).await;
    assert_eq!(status, 404);
}

// The smoke clone has to reach the bake it checks, which buyers cannot see yet.

#[tokio::test]
async fn the_smoke_clone_finds_the_unpromoted_bake() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let pool = &app.db_pool;
    let stack = "chan-smoke-resolve";
    legacy(pool, stack, "1.0.0", 100).await;
    baked(pool, stack, "1.1.0", 200).await;

    let unstable = |version: Option<&'static str>| async move {
        baked_snapshot::resolve_unstable(pool, stack, version, PROVIDER)
            .await
            .expect("resolve unstable")
            .map(|s| s.image_id)
    };
    assert_eq!(unstable(None).await, Some(200));
    assert_eq!(unstable(Some("1.1.0")).await, Some(200));
    assert_eq!(
        unstable(Some("1.0.0")).await,
        None,
        "a version that is not the unstable bake is not a fallback"
    );
    assert_eq!(
        served(pool, stack).await,
        Some(100),
        "buyers still get 1.0.0"
    );
}

async fn spawn_admin_app() -> Option<common::TestApp> {
    spawn_app_as("group_admin").await
}

async fn spawn_app_as(role: &str) -> Option<common::TestApp> {
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
    let auth = MockServer::start().await;
    let user = stacker::forms::user::User {
        id: common::USER_A_ID.to_string(),
        email: common::USER_A_EMAIL.to_string(),
        role: role.to_string(),
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

async fn clone_on_channel(app: &common::TestApp, stack: &str, channel: &str) -> (u16, Value) {
    let resp = reqwest::Client::new()
        .post(format!("{}/api/v1/deploy/clone", app.address))
        .bearer_auth("user-token")
        .json(&json!({
            "stack": stack,
            "domain": format!("{stack}-smoke.example.com"),
            "channel": channel,
        }))
        .send()
        .await
        .expect("request");
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn only_an_admin_may_clone_an_unpromoted_bake() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let stack = "chan-smoke-buyer";
    baked(&app.db_pool, stack, "1.0.0", 100).await;

    let (status, body) = clone_on_channel(&app, stack, "unstable").await;
    assert_eq!(
        status, 403,
        "a buyer cannot clone an unreleased bake: {body}"
    );
}

#[tokio::test]
async fn an_admin_smoke_clone_does_not_fall_back_to_the_released_snapshot() {
    let Some(app) = spawn_admin_app().await else {
        return;
    };
    let stack = "chan-smoke-admin";
    legacy(&app.db_pool, stack, "1.0.0", 100).await;

    let (status, body) = clone_on_channel(&app, stack, "unstable").await;
    assert_eq!(status, 404, "no unstable bake, so nothing to smoke: {body}");

    let (status, body) = clone_on_channel(&app, stack, "nightly").await;
    assert_eq!(status, 400, "an unknown channel is refused: {body}");
}

#[tokio::test]
async fn a_root_admin_may_smoke_clone_an_unpromoted_bake() {
    // `root` inherits `group_admin` in Casbin: the admin bake routes accept it,
    // so the smoke clone of the same admin's bake must too.
    let Some(app) = spawn_app_as("root").await else {
        return;
    };
    let stack = "chan-smoke-root";
    legacy(&app.db_pool, stack, "1.0.0", 100).await;

    let (status, body) = clone_on_channel(&app, stack, "unstable").await;
    assert_eq!(
        status, 404,
        "past the admin check, no unstable bake: {body}"
    );
}
