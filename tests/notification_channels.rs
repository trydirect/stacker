//! Integration tests for notification channels (WS-N).
//! Fixtures: config/shared-fixtures/features/notifications.feature.
//! Contracts: config/shared-fixtures/api-contracts/notifications-*.json.

mod common;

use reqwest::StatusCode;
use serde_json::{json, Value};
use stacker::configuration::get_configuration;
use stacker::services::VaultService;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::OnceCell;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

static APP: OnceCell<Option<VaultApp>> = OnceCell::const_new();

struct VaultApp {
    app: common::TwoUserTestApp,
    vault_server: MockServer,
    vault: VaultService,
}

async fn vault_app() -> Option<&'static VaultApp> {
    APP.get_or_init(|| async {
        let mut configuration = get_configuration().expect("Failed to get configuration");
        let vault_server = MockServer::start().await;
        configuration.vault.address = vault_server.uri();
        configuration.vault.token = "test-vault-token".to_string();
        configuration.vault.api_prefix = "v1".to_string();
        configuration.vault.ssh_key_path_prefix = Some("users".to_string());
        configuration.connectors.install_service =
            Some(stacker::connectors::InstallServiceConfig { enabled: false });
        let vault = VaultService::from_settings(&configuration.vault).expect("vault settings");
        // Skip gracefully when the test database is unreachable: the two-user
        // harness does not resolve PG* env overrides, so it can be unavailable
        // in some CI jobs (the tests run wherever a database is provided).
        let app = common::spawn_app_two_users_with_configuration(configuration).await?;
        Some(VaultApp {
            app,
            vault_server,
            vault,
        })
    })
    .await
    .as_ref()
}

fn regex_escape(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if ".+*?()|[]{}^$\\".contains(c) {
                format!("\\{}", c)
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Stub Vault GET/POST for one channel secret path.
async fn stub_vault_secret(vault_server: &MockServer, logical_path: &str, secret: &Value) {
    let escaped = regex_escape(logical_path);
    Mock::given(method("GET"))
        .and(path_regex(escaped.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": { "data": { "value": secret.to_string() } }
        })))
        .mount(vault_server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(escaped))
        .respond_with(ResponseTemplate::new(204))
        .mount(vault_server)
        .await;
}

fn secret_path(harness: &VaultApp, project_id: i32, name: &str) -> String {
    harness
        .vault
        .notification_secret_path(common::USER_A_ID, Some(project_id), name)
}

async fn create_project_channel(
    harness: &VaultApp,
    project_id: i32,
    name: &str,
    channel_type: &str,
    secret: &Value,
) -> reqwest::Response {
    stub_vault_secret(
        &harness.vault_server,
        &secret_path(harness, project_id, name),
        secret,
    )
    .await;
    reqwest::Client::new()
        .post(format!(
            "{}/api/v1/notifications/channels",
            harness.app.address
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({
            "name": name,
            "projectId": project_id,
            "channelType": channel_type,
            "secret": secret,
        }))
        .send()
        .await
        .expect("create channel request")
}

async fn body_of(response: reqwest::Response) -> Value {
    response.json().await.expect("response json")
}

#[tokio::test]
async fn create_project_channel_stores_secret_and_masks_target() {
    let Some(harness) = vault_app().await else {
        return;
    };
    let project_id = common::create_test_project(&harness.app.db_pool, common::USER_A_ID).await;
    let name = format!("ops-{}", uuid::Uuid::new_v4());
    let secret = json!({ "url": "https://hooks.slack.com/services/T0/B0/XXX" });

    let response = create_project_channel(harness, project_id, &name, "slack", &secret).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_of(response).await;
    let item = &body["item"];
    assert_eq!(item["name"], name.as_str());
    assert_eq!(item["projectId"], project_id);
    assert_eq!(item["targetHint"], "https://hooks.slack.com/***");
    let raw = body.to_string();
    assert!(!raw.contains("XXX"), "secret leaked in response: {raw}");
}

#[tokio::test]
async fn global_channel_requires_admin_role() {
    // Regression for code-review B1: a plain user must not be able to create
    // an organization-level channel.
    let Some(harness) = vault_app().await else {
        return;
    };
    let name = format!("global-{}", uuid::Uuid::new_v4());
    let secret = json!({ "url": "https://example.com/hook" });

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/v1/notifications/channels",
            harness.app.address
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "name": name, "channelType": "webhook", "secret": secret }))
        .send()
        .await
        .expect("create global channel request");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn duplicate_channel_name_is_rejected_without_touching_the_secret() {
    // Regression for code-review M1: a duplicate-name create must not
    // overwrite the existing channel's Vault secret.
    let Some(harness) = vault_app().await else {
        return;
    };
    let project_id = common::create_test_project(&harness.app.db_pool, common::USER_A_ID).await;
    let name = format!("dup-{}", uuid::Uuid::new_v4());

    let original_target = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&original_target)
        .await;
    let attacker_target = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&attacker_target)
        .await;

    let secret = json!({ "url": original_target.uri() });
    let first = create_project_channel(harness, project_id, &name, "slack", &secret).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let id = body_of(first).await["item"]["id"]
        .as_i64()
        .expect("channel id");

    let other_secret = json!({ "url": attacker_target.uri() });
    let second = create_project_channel(harness, project_id, &name, "slack", &other_secret).await;
    assert_eq!(second.status(), StatusCode::CONFLICT);

    // The first channel still delivers to its own target.
    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/v1/notifications/channels/{}/test",
            harness.app.address, id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("test request");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn another_user_cannot_touch_a_project_channel() {
    let Some(harness) = vault_app().await else {
        return;
    };
    let project_id = common::create_test_project(&harness.app.db_pool, common::USER_A_ID).await;
    let name = format!("mine-{}", uuid::Uuid::new_v4());
    let secret = json!({ "url": "https://example.com/hook" });

    let created = create_project_channel(harness, project_id, &name, "webhook", &secret).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let id = body_of(created).await["item"]["id"]
        .as_i64()
        .expect("channel id");

    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/v1/notifications/channels/{}",
            harness.app.address, id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&json!({ "enabled": false }))
        .send()
        .await
        .expect("patch request");
    // 404, not 403: the status code must not confirm existence (m6).
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rename_to_existing_name_is_rejected() {
    let Some(harness) = vault_app().await else {
        return;
    };
    let project_id = common::create_test_project(&harness.app.db_pool, common::USER_A_ID).await;
    let secret = json!({ "url": "https://example.com/hook" });

    let taken = format!("taken-{}", uuid::Uuid::new_v4());
    let first = create_project_channel(harness, project_id, &taken, "webhook", &secret).await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let other = format!("other-{}", uuid::Uuid::new_v4());
    let second = create_project_channel(harness, project_id, &other, "webhook", &secret).await;
    let id = body_of(second).await["item"]["id"]
        .as_i64()
        .expect("channel id");

    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/v1/notifications/channels/{}",
            harness.app.address, id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "name": taken }))
        .send()
        .await
        .expect("patch request");
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_message_is_delivered_and_recorded() {
    let Some(harness) = vault_app().await else {
        return;
    };
    let project_id = common::create_test_project(&harness.app.db_pool, common::USER_A_ID).await;
    let slack = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&slack)
        .await;

    let name = format!("slack-{}", uuid::Uuid::new_v4());
    let secret = json!({ "url": slack.uri() });
    let created = create_project_channel(harness, project_id, &name, "slack", &secret).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let id = body_of(created).await["item"]["id"]
        .as_i64()
        .expect("channel id");

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/v1/notifications/channels/{}/test",
            harness.app.address, id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("test request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_of(response).await;
    assert_eq!(body["item"]["status"], "sent");
    assert_eq!(body["item"]["eventType"], "channel.test");
    assert_eq!(body["item"]["attempts"], 1);

    // The dispatch is visible in the delivery log.
    let list = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/notifications/dispatches?channelId={}",
            harness.app.address, id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("dispatch list request");
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = body_of(list).await;
    let items = list_body["list"].as_array().expect("dispatch list");
    assert!(!items.is_empty());
    assert_eq!(items[0]["eventType"], "channel.test");
}

#[tokio::test]
async fn provider_rate_limit_is_retried_with_backoff() {
    let Some(harness) = vault_app().await else {
        return;
    };
    let project_id = common::create_test_project(&harness.app.db_pool, common::USER_A_ID).await;
    let slack = MockServer::start().await;
    let calls = AtomicUsize::new(0);
    Mock::given(method("POST"))
        .respond_with(move |_req: &wiremock::Request| {
            if calls.fetch_add(1, Ordering::SeqCst) < 2 {
                ResponseTemplate::new(429)
            } else {
                ResponseTemplate::new(200)
            }
        })
        .mount(&slack)
        .await;

    let name = format!("retry-{}", uuid::Uuid::new_v4());
    let secret = json!({ "url": slack.uri() });
    let created = create_project_channel(harness, project_id, &name, "slack", &secret).await;
    let id = body_of(created).await["item"]["id"]
        .as_i64()
        .expect("channel id");

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/v1/notifications/channels/{}/test",
            harness.app.address, id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("test request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_of(response).await;
    assert_eq!(body["item"]["status"], "sent");
    assert_eq!(body["item"]["attempts"], 3);
}

#[tokio::test]
async fn project_events_reach_only_subscribed_project_channels() {
    // N3 fan-out: the events filter is an allow-list and only the project's
    // own channels receive its events.
    let Some(harness) = vault_app().await else {
        return;
    };
    let project_id = common::create_test_project(&harness.app.db_pool, common::USER_A_ID).await;

    let fail_target = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&fail_target)
        .await;
    let ok_target = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&ok_target)
        .await;

    let failing = json!({ "url": fail_target.uri() });
    let succeeding = json!({ "url": ok_target.uri() });
    let name_fail = format!("sub-fail-{}", uuid::Uuid::new_v4());
    let name_ok = format!("sub-ok-{}", uuid::Uuid::new_v4());

    for (name, secret, events) in [
        (
            name_fail.as_str(),
            &failing,
            vec!["deploy.failed".to_string()],
        ),
        (
            name_ok.as_str(),
            &succeeding,
            vec!["deploy.succeeded".to_string()],
        ),
    ] {
        stub_vault_secret(
            &harness.vault_server,
            &secret_path(harness, project_id, name),
            secret,
        )
        .await;
        let response = reqwest::Client::new()
            .post(format!(
                "{}/api/v1/notifications/channels",
                harness.app.address
            ))
            .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
            .json(&json!({
                "name": name,
                "projectId": project_id,
                "channelType": "slack",
                "secret": secret,
                "events": events,
            }))
            .send()
            .await
            .expect("create channel request");
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let event = stacker::services::notification::build_event(
        "deploy.failed",
        Some(i64::from(project_id)),
        "deployment",
        "hash-1",
        "hash-1",
        "Deployment hash-1 failed: boom",
    );
    let dispatches = stacker::services::notification::dispatch_project_event(
        &harness.app.db_pool,
        None,
        &harness.vault,
        project_id,
        &event,
    )
    .await
    .expect("dispatch");

    // Only the deploy.failed subscriber is delivered to (mock expects assert
    // the actual HTTP calls).
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0].status, "sent");
}

#[tokio::test]
async fn permanent_provider_error_is_recorded_redacted() {
    let Some(harness) = vault_app().await else {
        return;
    };
    let project_id = common::create_test_project(&harness.app.db_pool, common::USER_A_ID).await;
    let slack = MockServer::start().await;
    let hook = format!("{}/services/SECRET-TOKEN", slack.uri());
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&slack)
        .await;

    let name = format!("fail-{}", uuid::Uuid::new_v4());
    let secret = json!({ "url": hook });
    let created = create_project_channel(harness, project_id, &name, "slack", &secret).await;
    let id = body_of(created).await["item"]["id"]
        .as_i64()
        .expect("channel id");

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/v1/notifications/channels/{}/test",
            harness.app.address, id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("test request");
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_of(response).await;
    assert_eq!(body["item"]["status"], "failed");
    assert_eq!(body["item"]["attempts"], 1);
    let raw = body.to_string();
    assert!(
        !raw.contains("SECRET-TOKEN"),
        "secret leaked in error: {raw}"
    );
}
