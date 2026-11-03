/// Integration tests for `GET /api/templates/mine`.
///
/// These are HTTP-level tests that spin up the full Actix server (via `spawn_app`)
/// including mock OAuth, real Postgres with migrations, and Casbin RBAC.
///
/// Why they exist
/// --------------
/// `tests/marketplace_integration.rs` only tests connector-level logic (DeploymentValidator,
/// MockUserServiceConnector). It never makes an actual HTTP request, so it cannot detect
/// routing or authorization failures.  The live endpoint was returning a 404 produced by
/// an external proxy / old production binary — not by Stacker itself.  These tests confirm:
///   - The route is registered and reachable
///   - Casbin grants `group_user` access to the endpoint
///   - Stacker responds with 403 (not 404) when auth is missing, proving any 404 in
///     production is external
mod common;

use reqwest::StatusCode;

use std::sync::{Mutex, OnceLock};
use tokio::sync::OnceCell;

/// Serialises the tests in this file.
///
/// Every test authenticates as the same mock user (`test_user_id`) against the
/// same database, and one of them asserts that the user has *no* templates
/// while another seeds one. Run concurrently, the seeding test makes the
/// empty-list test see a row and fail; it failed on roughly every parallel run
/// and passed under `--test-threads=1`, which reads as noise rather than a bug.
/// Same approach as `marketplace_creator_vendor_profile_onboarding_complete`.
///
/// `unwrap_or_else(|e| e.into_inner())` because a failing test poisons a
/// `std::sync::Mutex`, and one failure should not cascade into the others.
fn user_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// A fresh `PgPool` per test, bound to that test's own runtime. Sharing one
// pool across `#[tokio::test]` functions hands out connections belonging to a
// runtime that has already been dropped, and `acquire()` then blocks for the
// full 120s timeout and fails with `PoolTimedOut`. The server still starts
// once; only the pool is per test.
static APP_CONFIG: OnceCell<common::TestAppConfig> = OnceCell::const_new();

async fn app() -> common::TestApp {
    common::get_or_init_app_fresh(&APP_CONFIG)
        .await
        .expect("Failed to start test app")
}

// Any non-empty token works: the mock auth server (spawned by common::spawn_app)
// validates all Bearer tokens and returns role = "group_user", id = "test_user_id".
const BEARER_TOKEN: &str = "test-bearer-token";

/// Authenticated user with no templates receives a 200 with an empty list.
#[tokio::test]
async fn mine_returns_empty_list_for_new_user() {
    let _user = user_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = app().await;
    let client = reqwest::Client::new();

    // Establish the precondition instead of assuming it. Every test in this
    // file authenticates as the same mock user against the same database, and
    // `mine_returns_only_the_authenticated_users_templates` seeds a row for
    // that user which outlives it. Serialising the tests is not enough: if the
    // seeding test simply runs first, the row is still there. Deleting here
    // makes this test independent of the order it runs in.
    sqlx::query("DELETE FROM stack_template WHERE creator_user_id = 'test_user_id'")
        .execute(&app.db_pool)
        .await
        .expect("Failed to clear templates for the test user");

    let response = client
        .get(format!("{}/api/templates/mine", app.address))
        .bearer_auth(BEARER_TOKEN)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(StatusCode::OK, response.status());

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");
    let list = body
        .get("list")
        .expect("Response body should contain 'list' field");
    assert!(list.is_array(), "'list' should be a JSON array");
    assert_eq!(
        0,
        list.as_array().unwrap().len(),
        "Newly authenticated user should have no templates"
    );
}

/// Authenticated user sees their own templates, not other users' templates.
#[tokio::test]
async fn mine_returns_only_the_authenticated_users_templates() {
    let _user = user_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = app().await;
    let client = reqwest::Client::new();

    // Seed one template for the mock user (id = "test_user_id" per common::mock_auth).
    sqlx::query(
        r#"INSERT INTO stack_template (creator_user_id, name, slug, status, tags, tech_stack)
           VALUES ('test_user_id', 'My Test Stack', 'my-test-stack', 'draft', '[]', '{}')"#,
    )
    .execute(&app.db_pool)
    .await
    .expect("Failed to seed template for test user");

    // Seed one template for a different user — should NOT appear in the response.
    sqlx::query(
        r#"INSERT INTO stack_template (creator_user_id, name, slug, status, tags, tech_stack)
           VALUES ('other_user_id', 'Other Stack', 'other-stack', 'draft', '[]', '{}')"#,
    )
    .execute(&app.db_pool)
    .await
    .expect("Failed to seed template for other user");

    let response = client
        .get(format!("{}/api/templates/mine", app.address))
        .bearer_auth(BEARER_TOKEN)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(StatusCode::OK, response.status());

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");
    let list = body["list"]
        .as_array()
        .expect("'list' should be a JSON array");

    assert_eq!(
        1,
        list.len(),
        "Should return exactly the authenticated user's template"
    );
    assert_eq!(
        "my-test-stack",
        list[0]["slug"].as_str().unwrap_or_default(),
        "Returned template slug should match seeded value"
    );
}

/// Unauthenticated request (no Authorization header) must return 403 Forbidden.
///
/// This assertion also serves as evidence that the Stacker server itself returns 403
/// for missing auth — *not* 404.  Any 404 seen in production therefore originates
/// from an external reverse proxy or an outdated server binary, not from this route.
#[tokio::test]
async fn mine_returns_forbidden_without_authorization_header() {
    let _user = user_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/templates/mine", app.address))
        // Intentionally omit .bearer_auth(…)
        .send()
        .await
        .expect("Failed to send request");

    let status = response.status();
    assert_eq!(
        StatusCode::FORBIDDEN,
        status,
        "Missing auth should yield 403 Forbidden — Stacker never returns 404 for this route"
    );
}
