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

#[tokio::test]
async fn installation_event_is_recorded() {
    let app = app().await;
    stub_webhook_secret(&app.vault_server).await;

    let installation_id = 9000 + uuid::Uuid::new_v4().as_u128() % 1000;
    let body = json!({
        "action": "created",
        "installation": { "id": installation_id, "account": { "login": "alice" } },
        "repositories": [ { "full_name": "alice/shop" } ]
    });
    let response = send_webhook(
        app,
        "installation",
        &uuid::Uuid::new_v4().to_string(),
        &body,
        WEBHOOK_SECRET,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM github_installation WHERE installation_id = $1",
    )
    .bind(installation_id as i64)
    .fetch_one(&app.db_pool)
    .await
    .expect("count installations");
    assert_eq!(count, 1);
}

// --- Repo Connect API (G4) -----------------------------------------------

async fn connect_repo(
    app: &common::TestAppWithVault,
    project_id: i32,
    repo: &str,
    token: &str,
) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{}/api/v1/github/repos", app.address))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "projectId": project_id,
            "installationId": 42,
            "repo": repo,
            "branch": "main",
            "watchPaths": ["src/*"],
            "autodeploy": true,
        }))
        .send()
        .await
        .expect("connect repo request")
}

#[tokio::test]
async fn repo_connect_crud_roundtrip() {
    let app = app().await;
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let repo_name = format!("alice/shop-{}", uuid::Uuid::new_v4());

    let created = connect_repo(app, project_id, &repo_name, common::USER_A_TOKEN).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_body: Value = created.json().await.expect("json");
    let repo_id = created_body["item"]["id"].as_i64().expect("repo id");
    assert_eq!(created_body["item"]["projectId"], project_id);

    // Duplicate repo+branch is rejected.
    let duplicate = connect_repo(app, project_id, &repo_name, common::USER_A_TOKEN).await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);

    // Update switches auto-deploy off.
    let updated = reqwest::Client::new()
        .patch(format!("{}/api/v1/github/repos/{}", app.address, repo_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "autodeploy": false }))
        .send()
        .await
        .expect("patch request");
    assert_eq!(updated.status(), StatusCode::OK);
    let updated_body: Value = updated.json().await.expect("json");
    assert_eq!(updated_body["item"]["autodeploy"], false);

    // List shows it; delete removes it.
    let list = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/github/repos?projectId={}",
            app.address, project_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("list request");
    let list_body: Value = list.json().await.expect("json");
    assert_eq!(list_body["list"].as_array().expect("list").len(), 1);

    let deleted = reqwest::Client::new()
        .delete(format!("{}/api/v1/github/repos/{}", app.address, repo_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("delete request");
    assert_eq!(deleted.status(), StatusCode::OK);
}

#[tokio::test]
async fn repo_connect_requires_project_ownership() {
    let app = app().await;
    // A project owned by another user: User A must not connect repos to it.
    let other_project = common::create_test_project(&app.db_pool, common::USER_B_ID).await;
    let repo_name = format!("alice/shop-{}", uuid::Uuid::new_v4());

    let response = connect_repo(app, other_project, &repo_name, common::USER_A_TOKEN).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
