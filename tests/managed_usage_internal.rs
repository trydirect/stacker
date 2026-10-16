//! Internal endpoints the User Service calls for the Managed server of a
//! Quick Deploy. Contract: shared-fixtures/api-contracts/managed-server-billing.json

mod common;

use serde_json::{json, Value};
use stacker::db::managed_usage::{self, NewManagedUsage};

const KEY: &str = "test-internal-key";

fn set_key() {
    std::env::set_var("INTERNAL_SERVICES_ACCESS_KEY", KEY);
}

async fn pending(app: &common::TestApp, hash: &str, user_id: &str) {
    managed_usage::insert_pending(
        &app.db_pool,
        &NewManagedUsage {
            deployment_hash: hash.to_string(),
            user_id: user_id.to_string(),
            stack: "local-ai-packaged".to_string(),
            server_type: "cpx32".to_string(),
            daily_rate_minor: 86,
            monthly_cap_minor: 2600,
        },
    )
    .await
    .expect("insert");
}

async fn post(app: &common::TestApp, path: &str, key: Option<&str>, body: Value) -> (u16, Value) {
    let mut req = reqwest::Client::new()
        .post(format!(
            "{}/api/internal/managed-usage/{}",
            app.address, path
        ))
        .json(&body);
    if let Some(key) = key {
        req = req.header("x-internal-key", key);
    }
    let resp = req.send().await.expect("request");
    let status = resp.status().as_u16();
    let body = resp.json().await.unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn internal_calls_without_the_key_are_refused() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    set_key();
    pending(&app, "dep-nokey", "u-1").await;
    for path in [
        "dep-nokey/start",
        "dep-nokey/fail",
        "dep-nokey/stop",
        "suspend",
    ] {
        let (status, _) = post(&app, path, None, json!({"user_id": "u-1"})).await;
        assert_eq!(status, 401, "{path} without a key");
        let (status, _) = post(&app, path, Some("wrong"), json!({"user_id": "u-1"})).await;
        assert_eq!(status, 401, "{path} with a wrong key");
    }
    let row = managed_usage::fetch_by_hash(&app.db_pool, "dep-nokey")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "pending");
}

#[tokio::test]
async fn start_makes_a_pending_deploy_billed() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    set_key();
    pending(&app, "dep-go", "u-1").await;

    let (status, body) = post(&app, "dep-go/start", Some(KEY), json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "active");

    let (status, _) = post(&app, "dep-go/start", Some(KEY), json!({})).await;
    assert_eq!(status, 409, "starting twice");
    let (status, _) = post(&app, "dep-unknown/start", Some(KEY), json!({})).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn fail_closes_a_pending_deploy() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    set_key();
    pending(&app, "dep-broke", "u-1").await;

    let (status, body) = post(
        &app,
        "dep-broke/fail",
        Some(KEY),
        json!({"error": "post-deploy setup failed"}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "closed");
    assert_eq!(body["outcome"], "deploy_failed");
}

#[tokio::test]
async fn suspend_stops_billing_every_active_server_of_the_buyer() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    set_key();
    for hash in ["dep-a", "dep-b"] {
        pending(&app, hash, "u-owes").await;
        managed_usage::start(&app.db_pool, hash).await.unwrap();
    }
    pending(&app, "dep-c", "u-owes").await; // still deploying: not suspended
    pending(&app, "dep-d", "u-pays").await;
    managed_usage::start(&app.db_pool, "dep-d").await.unwrap();

    let (status, body) = post(&app, "suspend", Some(KEY), json!({"user_id": "u-owes"})).await;
    assert_eq!(status, 200, "{body}");
    let mut suspended: Vec<String> = body["suspended"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    suspended.sort();
    assert_eq!(suspended, vec!["dep-a", "dep-b"]);

    let other = managed_usage::fetch_by_hash(&app.db_pool, "dep-d")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other.status, "active");

    let (status, _) = post(&app, "suspend", Some(KEY), json!({})).await;
    assert_eq!(status, 400, "user_id is required");
}

#[tokio::test]
async fn stop_ends_the_usage_of_a_deleted_deployment_and_returns_the_vm() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    set_key();
    pending(&app, "dep-del", "u-1").await;
    managed_usage::start(&app.db_pool, "dep-del").await.unwrap();
    managed_usage::set_server_id(&app.db_pool, "dep-del", 777)
        .await
        .unwrap();

    let (status, body) = post(&app, "dep-del/stop", Some(KEY), json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "stopped");
    assert_eq!(body["server_id"], 777);

    let (status, _) = post(&app, "dep-del/stop", Some(KEY), json!({})).await;
    assert_eq!(status, 409, "stopping twice");
    let (status, _) = post(&app, "dep-none/stop", Some(KEY), json!({})).await;
    assert_eq!(status, 404);
}
