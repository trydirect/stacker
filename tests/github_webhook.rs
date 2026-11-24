//! Integration tests for the GitHub App webhook receiver (WS-G G2/G3).
//! Plan: config/PLAN-github-webhooks.md.
//! Contract: config/shared-fixtures/api-contracts/github-webhooks.json.

mod common;

use hmac::{Hmac, Mac};
use reqwest::StatusCode;
use serde_json::{json, Value};
use sha2::Sha256;
use stacker::db;
use tokio::sync::OnceCell;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

static APP: OnceCell<common::TestAppWithVault> = OnceCell::const_new();

const WEBHOOK_SECRET: &str = "test-github-webhook-secret";

async fn app() -> &'static common::TestAppWithVault {
    common::get_or_init_vault_app(&APP)
        .await
        .expect("Failed to start test app")
}

fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac");
    mac.update(body);
    let hex: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    format!("sha256={}", hex)
}

async fn stub_webhook_secret(vault_server: &MockServer) {
    Mock::given(method("GET"))
        .and(path_regex("github/webhook_secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "data": { "value": WEBHOOK_SECRET } }
        })))
        .mount(vault_server)
        .await;
}

async fn send_webhook(
    app: &common::TestAppWithVault,
    event: &str,
    delivery_id: &str,
    body: &Value,
    secret: &str,
) -> reqwest::Response {
    let raw = serde_json::to_vec(body).expect("serialize body");
    let signature = sign(secret, &raw);
    reqwest::Client::new()
        .post(format!("{}/api/v1/github/webhook", app.address))
        .header("X-GitHub-Event", event)
        .header("X-GitHub-Delivery", delivery_id)
        .header("X-Hub-Signature-256", signature)
        .header("Content-Type", "application/json")
        .body(raw)
        .send()
        .await
        .expect("webhook request")
}

fn push_body(repo: &str, changed: &[&str]) -> Value {
    json!({
        "ref": "refs/heads/main",
        "after": "abc123",
        "repository": { "full_name": repo },
        "commits": [ { "added": changed, "modified": [], "removed": [] } ]
    })
}

#[tokio::test]
async fn invalid_signature_is_rejected() {
    let app = app().await;
    stub_webhook_secret(&app.vault_server).await;

    let response = send_webhook(
        app,
        "push",
        &uuid::Uuid::new_v4().to_string(),
        &push_body("alice/shop", &["src/a.rs"]),
        "wrong-secret",
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn duplicate_delivery_is_skipped() {
    let app = app().await;
    stub_webhook_secret(&app.vault_server).await;

    let delivery_id = uuid::Uuid::new_v4().to_string();
    let body = json!({ "zen": "Design for failure." });

    let first = send_webhook(app, "ping", &delivery_id, &body, WEBHOOK_SECRET).await;
    assert_eq!(first.status(), StatusCode::OK);
    let first_body: Value = first.json().await.expect("json");
    assert_eq!(first_body["item"]["duplicate"], Value::Null);

    let second = send_webhook(app, "ping", &delivery_id, &body, WEBHOOK_SECRET).await;
    assert_eq!(second.status(), StatusCode::OK);
    let second_body: Value = second.json().await.expect("json");
    assert_eq!(second_body["item"]["duplicate"], true);
}

#[tokio::test]
async fn push_matches_repo_branch_and_watch_paths() {
    let app = app().await;
    stub_webhook_secret(&app.vault_server).await;

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let repo_name = format!("alice/shop-{}", uuid::Uuid::new_v4());
    db::github::insert_repo(
        &app.db_pool,
        common::USER_A_ID,
        42,
        Some(project_id),
        &repo_name,
        "main",
        &["src/*".to_string()],
        true,
    )
    .await
    .expect("insert repo");

    let response = send_webhook(
        app,
        "push",
        &uuid::Uuid::new_v4().to_string(),
        &push_body(&repo_name, &["src/a.rs"]),
        WEBHOOK_SECRET,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("json");
    let matched = body["item"]["matched"].as_array().expect("matched list");
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0]["projectId"], project_id);
}

#[tokio::test]
async fn push_outside_watch_paths_does_not_match() {
    let app = app().await;
    stub_webhook_secret(&app.vault_server).await;

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let repo_name = format!("alice/docs-{}", uuid::Uuid::new_v4());
    db::github::insert_repo(
        &app.db_pool,
        common::USER_A_ID,
        42,
        Some(project_id),
        &repo_name,
        "main",
        &["src/*".to_string()],
        true,
    )
    .await
    .expect("insert repo");

    let response = send_webhook(
        app,
        "push",
        &uuid::Uuid::new_v4().to_string(),
        &push_body(&repo_name, &["docs/readme.md"]),
        WEBHOOK_SECRET,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("json");
    let matched = body["item"]["matched"].as_array().expect("matched list");
    assert!(matched.is_empty());
}
