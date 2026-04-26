mod common;

use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

async fn insert_approved_template(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query(
        r#"INSERT INTO stack_template (
            creator_user_id,
            name,
            slug,
            status,
            tags,
            tech_stack,
            deploy_count
        )
        VALUES ($1, $2, $3, 'approved', '[]'::jsonb, '{}'::jsonb, 0)
        RETURNING id"#,
    )
    .bind("creator-1")
    .bind("Deploy Complete Template")
    .bind(format!("deploy-complete-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("Failed to insert approved marketplace template")
    .get::<Uuid, _>("id")
}

#[tokio::test]
async fn deploy_complete_validates_purchase_token_increments_count_and_returns_template_id() {
    let _env_lock = env_lock().lock().expect("env lock should be available");
    let _token_guard = EnvGuard::set("STACKER_SERVICE_TOKEN", "test-service-token");

    let user_service = MockServer::start().await;
    let mut configuration = stacker::configuration::get_configuration()
        .expect("Failed to get configuration");
    configuration.user_service_url = user_service.uri();

    let app = match common::spawn_app_with_configuration(configuration).await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_approved_template(&app.db_pool).await;

    Mock::given(method("POST"))
        .and(path("/marketplace/purchase-token/validate"))
        .and(header("authorization", "Bearer test-service-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "valid": true,
            "product_id": "product-1",
            "user_id": 42,
            "stack_id": template_id.to_string(),
        })))
        .mount(&user_service)
        .await;

    let client = Client::new();
    let callback_url = format!("{}/api/v1/marketplace/deploy-complete", app.address);
    let callback_body = json!({
        "deployment_hash": "deployment-hash-1",
        "purchase_token": "purchase-token-1",
        "server_ip": "203.0.113.10",
        "stack_id": template_id.to_string(),
    });

    let response = client
        .post(&callback_url)
        .header("X-Stacker-Service-Token", "test-service-token")
        .json(&callback_body)
        .send()
        .await
        .expect("Failed to send deploy-complete request");

    assert_eq!(StatusCode::OK, response.status());
    let body: Value = response
        .json()
        .await
        .expect("deploy-complete response should be valid JSON");
    assert_eq!(template_id.to_string(), body["item"]["template_id"]);
    assert_eq!(true, body["item"]["deploy_count_incremented"]);

    let deploy_count = sqlx::query("SELECT deploy_count FROM stack_template WHERE id = $1")
        .bind(template_id)
        .fetch_one(&app.db_pool)
        .await
        .expect("Template should exist")
        .get::<Option<i32>, _>("deploy_count");

    assert_eq!(Some(1), deploy_count);

    let duplicate_response = client
        .post(&callback_url)
        .header("X-Stacker-Service-Token", "test-service-token")
        .json(&callback_body)
        .send()
        .await
        .expect("Failed to send duplicate deploy-complete request");

    assert_eq!(StatusCode::OK, duplicate_response.status());
    let duplicate_body: Value = duplicate_response
        .json()
        .await
        .expect("duplicate deploy-complete response should be valid JSON");
    assert_eq!(template_id.to_string(), duplicate_body["item"]["template_id"]);
    assert_eq!(false, duplicate_body["item"]["deploy_count_incremented"]);

    let deploy_count_after_duplicate =
        sqlx::query("SELECT deploy_count FROM stack_template WHERE id = $1")
            .bind(template_id)
            .fetch_one(&app.db_pool)
            .await
            .expect("Template should exist")
            .get::<Option<i32>, _>("deploy_count");

    assert_eq!(Some(1), deploy_count_after_duplicate);
}
