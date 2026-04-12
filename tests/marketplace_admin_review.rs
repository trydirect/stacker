mod common;

use chrono::{Duration, Utc};
use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::{Mutex, OnceLock};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn create_admin_jwt() -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let header = json!({"alg": "HS256", "typ": "JWT"});
    let payload = json!({
        "role": "admin_service",
        "email": "ops@test.com",
        "exp": (Utc::now() + Duration::minutes(30)).timestamp(),
    });

    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string());

    format!("{}.{}.{}", header_b64, payload_b64, "test_signature")
}

async fn insert_template(
    pool: &sqlx::PgPool,
    creator_user_id: &str,
    slug: &str,
    status: &str,
) -> String {
    sqlx::query(
        r#"INSERT INTO stack_template (
            creator_user_id,
            creator_name,
            name,
            slug,
            status,
            tags,
            tech_stack
        )
        VALUES ($1, 'Creator Example', 'Review Template', $2, $3, '[]'::jsonb, '{}'::jsonb)
        RETURNING id"#,
    )
    .bind(creator_user_id)
    .bind(slug)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("Failed to insert template")
    .get::<uuid::Uuid, _>("id")
    .to_string()
}

struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            std::env::set_var(self.key, previous);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[tokio::test]
async fn admin_can_mark_template_needs_changes_and_creator_can_see_reason() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "needs-changes-review-template",
        "submitted",
    )
    .await;

    let admin_response = reqwest::Client::new()
        .post(format!(
            "{}/api/admin/templates/{}/needs-changes",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "reason": "Please document the required Hetzner bare metal prerequisites."
        }))
        .send()
        .await
        .expect("Failed to send admin needs-changes request");

    assert_eq!(StatusCode::OK, admin_response.status());

    let template_status = sqlx::query_scalar::<_, String>(
        r#"SELECT status FROM stack_template WHERE id = $1::uuid"#,
    )
    .bind(uuid::Uuid::parse_str(&template_id).expect("template id should be a uuid"))
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch updated template status");

    assert_eq!("needs_changes", template_status);

    let reviews_response = reqwest::Client::new()
        .get(format!("{}/api/templates/{}/reviews", app.address, template_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to fetch creator reviews");

    assert_eq!(StatusCode::OK, reviews_response.status());

    let body: Value = reviews_response
        .json()
        .await
        .expect("reviews response should be valid JSON");
    let latest_review = &body["list"][0];

    assert_eq!("needs_changes", latest_review["decision"]);
    assert_eq!(
        "Please document the required Hetzner bare metal prerequisites.",
        latest_review["review_reason"]
    );
}

#[tokio::test]
async fn admin_cannot_mark_approved_template_as_needs_changes() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "approved-template-needs-changes-blocked",
        "approved",
    )
    .await;

    let admin_response = reqwest::Client::new()
        .post(format!(
            "{}/api/admin/templates/{}/needs-changes",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "reason": "Please update the deployment guide."
        }))
        .send()
        .await
        .expect("Failed to send admin needs-changes request");

    assert_eq!(StatusCode::BAD_REQUEST, admin_response.status());

    let template_status = sqlx::query_scalar::<_, String>(
        r#"SELECT status FROM stack_template WHERE id = $1::uuid"#,
    )
    .bind(uuid::Uuid::parse_str(&template_id).expect("template id should be a uuid"))
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch template status");

    assert_eq!("approved", template_status);
}

#[tokio::test]
async fn admin_approval_sends_template_published_webhook() {
    let _env_lock = env_lock().lock().expect("env lock should be available");
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let mock_user_service = MockServer::start().await;
    let _url_server_user = EnvGuard::set("URL_SERVER_USER", &mock_user_service.uri());
    let _user_service_url = EnvGuard::set("USER_SERVICE_URL", &mock_user_service.uri());
    let _user_service_base_url = EnvGuard::set("USER_SERVICE_BASE_URL", &mock_user_service.uri());
    let _stacker_service_token = EnvGuard::set("STACKER_SERVICE_TOKEN", "stacker-test-token");

    Mock::given(method("POST"))
        .and(path("/marketplace/sync"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "message": "ok",
            "product_id": null
        })))
        .mount(&mock_user_service)
        .await;

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "published-webhook-template",
        "submitted",
    )
    .await;

    let admin_response = reqwest::Client::new()
        .post(format!(
            "{}/api/admin/templates/{}/approve",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "decision": "approved",
            "reason": "Looks good."
        }))
        .send()
        .await
        .expect("Failed to send admin approval request");

    assert_eq!(StatusCode::OK, admin_response.status());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let requests = mock_user_service
        .received_requests()
        .await
        .expect("Should capture webhook request");
    let webhook_request = requests
        .iter()
        .find(|request| request.url.path() == "/marketplace/sync")
        .expect("Approval should send marketplace webhook");
    let payload: Value =
        serde_json::from_slice(&webhook_request.body).expect("Webhook body should be valid JSON");

    assert_eq!("template_published", payload["action"]);
    assert_eq!(template_id, payload["stack_template_id"]);
    assert_eq!("published-webhook-template", payload["code"]);
}

#[tokio::test]
async fn admin_rejection_sends_template_review_rejected_webhook() {
    let _env_lock = env_lock().lock().expect("env lock should be available");
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let mock_user_service = MockServer::start().await;
    let _url_server_user = EnvGuard::set("URL_SERVER_USER", &mock_user_service.uri());
    let _user_service_url = EnvGuard::set("USER_SERVICE_URL", &mock_user_service.uri());
    let _user_service_base_url = EnvGuard::set("USER_SERVICE_BASE_URL", &mock_user_service.uri());
    let _stacker_service_token = EnvGuard::set("STACKER_SERVICE_TOKEN", "stacker-test-token");

    Mock::given(method("POST"))
        .and(path("/marketplace/sync"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "message": "ok",
            "product_id": null
        })))
        .mount(&mock_user_service)
        .await;

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "review-rejected-template",
        "submitted",
    )
    .await;

    let admin_response = reqwest::Client::new()
        .post(format!(
            "{}/api/admin/templates/{}/reject",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "decision": "rejected",
            "reason": "The submission does not meet marketplace quality standards yet."
        }))
        .send()
        .await
        .expect("Failed to send admin rejection request");

    assert_eq!(StatusCode::OK, admin_response.status());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let requests = mock_user_service
        .received_requests()
        .await
        .expect("Should capture webhook request");
    let webhook_request = requests
        .iter()
        .find(|request| request.url.path() == "/marketplace/sync")
        .expect("Rejection should send marketplace webhook");
    let payload: Value =
        serde_json::from_slice(&webhook_request.body).expect("Webhook body should be valid JSON");

    assert_eq!("template_review_rejected", payload["action"]);
    assert_eq!(template_id, payload["stack_template_id"]);
    assert_eq!(
        "The submission does not meet marketplace quality standards yet.",
        payload["review_reason"]
    );
}

#[tokio::test]
async fn admin_unapprove_sends_template_unpublished_webhook() {
    let _env_lock = env_lock().lock().expect("env lock should be available");
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let mock_user_service = MockServer::start().await;
    let _url_server_user = EnvGuard::set("URL_SERVER_USER", &mock_user_service.uri());
    let _user_service_url = EnvGuard::set("USER_SERVICE_URL", &mock_user_service.uri());
    let _user_service_base_url = EnvGuard::set("USER_SERVICE_BASE_URL", &mock_user_service.uri());
    let _stacker_service_token = EnvGuard::set("STACKER_SERVICE_TOKEN", "stacker-test-token");

    Mock::given(method("POST"))
        .and(path("/marketplace/sync"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "message": "ok",
            "product_id": null
        })))
        .mount(&mock_user_service)
        .await;

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "unpublished-template",
        "approved",
    )
    .await;

    let admin_response = reqwest::Client::new()
        .post(format!(
            "{}/api/admin/templates/{}/unapprove",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "reason": "Temporarily hidden from the marketplace."
        }))
        .send()
        .await
        .expect("Failed to send admin unapprove request");

    assert_eq!(StatusCode::OK, admin_response.status());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let requests = mock_user_service
        .received_requests()
        .await
        .expect("Should capture webhook request");
    let webhook_request = requests
        .iter()
        .find(|request| request.url.path() == "/marketplace/sync")
        .expect("Unapprove should send marketplace webhook");
    let payload: Value =
        serde_json::from_slice(&webhook_request.body).expect("Webhook body should be valid JSON");

    assert_eq!("template_unpublished", payload["action"]);
    assert_eq!(template_id, payload["stack_template_id"]);
}