mod common;

use tokio::sync::OnceCell;

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

// test me: cargo t --test cloud -- --nocapture --show-output
#[tokio::test]
async fn list() {
    let app = app().await;
    let client = reqwest::Client::new(); // client

    let response = client
        .get(format!("{}/cloud", &app.address))
        .header("Authorization", "Bearer test_token")
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
}

// test me: cargo t --test cloud add_cloud -- --nocapture --show-output
#[tokio::test]
async fn add_cloud() {
    let app = app().await;
    let client = reqwest::Client::new(); // client

    let response = client
        .post(format!("{}/cloud", &app.address))
        .header("Authorization", "Bearer test_token")
        .header("Content-Type", "application/json")
        .body(r#"{"name":"Test Cloud","provider":"hetzner","cloud_token":"test_token"}"#)
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
}
