mod common;

use reqwest::{Client, Response, StatusCode};
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn create_template(
    client: &Client,
    address: &str,
    token: &str,
    name: &str,
    slug: &str,
) -> Response {
    create_template_with_body(
        client,
        address,
        token,
        json!({
            "name": name,
            "slug": slug,
        }),
    )
    .await
}

async fn create_template_with_body(
    client: &Client,
    address: &str,
    token: &str,
    body: Value,
) -> Response {
    client
        .post(format!("{}/api/templates", address))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .expect("Failed to send template create request")
}

async fn list_my_templates(client: &Client, address: &str, token: &str) -> Value {
    client
        .get(format!("{}/api/templates/mine", address))
        .bearer_auth(token)
        .send()
        .await
        .expect("Failed to fetch my templates")
        .json()
        .await
        .expect("Templates response should be valid JSON")
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
async fn create_template_with_duplicate_slug_by_different_user_returns_409() {
    let app = match common::spawn_app_two_users().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let first = create_template(
        &client,
        &app.address,
        common::USER_A_TOKEN,
        "User A Template",
        "duplicate-marketplace-slug",
    )
    .await;
    assert_eq!(StatusCode::CREATED, first.status());

    let second = create_template(
        &client,
        &app.address,
        common::USER_B_TOKEN,
        "User B Template",
        "duplicate-marketplace-slug",
    )
    .await;
    assert_eq!(StatusCode::CONFLICT, second.status());

    let body: Value = second
        .json()
        .await
        .expect("Conflict response should be valid JSON");
    let message = body["message"]
        .as_str()
        .expect("Conflict response should include a message");
    assert!(
        message.contains("already in use"),
        "Conflict message should explain the slug collision: {message}"
    );
    assert!(
        message.contains("duplicate-marketplace-slug"),
        "Conflict message should include the conflicting slug: {message}"
    );
}

#[tokio::test]
async fn create_template_same_slug_same_user_updates_existing_template() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let first = create_template(
        &client,
        &app.address,
        "test-bearer-token",
        "Original Template",
        "same-user-upsert-slug",
    )
    .await;
    assert_eq!(StatusCode::CREATED, first.status());
    let first_body: Value = first
        .json()
        .await
        .expect("Create response should be valid JSON");
    let first_id = first_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id")
        .to_string();

    let second = create_template(
        &client,
        &app.address,
        "test-bearer-token",
        "Renamed Template",
        "same-user-upsert-slug",
    )
    .await;
    assert_eq!(StatusCode::CREATED, second.status());
    let second_body: Value = second
        .json()
        .await
        .expect("Second create response should be valid JSON");
    let second_id = second_body["item"]["id"]
        .as_str()
        .expect("Updated template should include an id");
    assert_eq!(
        first_id, second_id,
        "Same-user duplicate slug should upsert"
    );
    assert_eq!(
        Some("Renamed Template"),
        second_body["item"]["name"].as_str(),
        "Second request should update template metadata"
    );
}

#[tokio::test]
async fn create_template_returns_infrastructure_requirements_when_provided() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Requirements Template",
            "slug": "requirements-template",
            "infrastructure_requirements": {
                "supported_clouds": ["hetzner", "aws"],
                "supported_os": ["ubuntu-22.04"],
                "min_ram_mb": 2048,
                "min_disk_gb": 20,
                "min_cpu_cores": 2
            }
        }),
    )
    .await;

    assert_eq!(StatusCode::CREATED, response.status());

    let body: Value = response
        .json()
        .await
        .expect("Create response should be valid JSON");

    assert_eq!(
        json!({
            "supported_clouds": ["hetzner", "aws"],
            "supported_os": ["ubuntu-22.04"],
            "min_ram_mb": 2048,
            "min_disk_gb": 20,
            "min_cpu_cores": 2
        }),
        body["item"]["infrastructure_requirements"]
    );
}

#[tokio::test]
async fn update_template_persists_infrastructure_requirements() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let create_response = create_template(
        &client,
        &app.address,
        "test-bearer-token",
        "Updatable Template",
        "updatable-template",
    )
    .await;
    assert_eq!(StatusCode::CREATED, create_response.status());
    let create_body: Value = create_response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = create_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id");

    let update_response = client
        .put(format!("{}/api/templates/{}", app.address, template_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "infrastructure_requirements": {
                "supported_clouds": ["digitalocean"],
                "min_ram_mb": 1024
            }
        }))
        .send()
        .await
        .expect("Failed to update template");
    assert_eq!(StatusCode::OK, update_response.status());

    let mine = list_my_templates(&client, &app.address, "test-bearer-token").await;
    let templates = mine["list"]
        .as_array()
        .expect("Mine response should contain a list");
    let template = templates
        .iter()
        .find(|template| template["slug"] == "updatable-template")
        .expect("Updated template should be present in my templates");

    assert_eq!(
        json!({
            "supported_clouds": ["digitalocean"],
            "min_ram_mb": 1024
        }),
        template["infrastructure_requirements"]
    );
}

#[tokio::test]
async fn submit_template_sends_template_submitted_webhook() {
    let _env_lock = env_lock().lock().expect("env lock should be available");
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();
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

    let create_response = create_template(
        &client,
        &app.address,
        "test-bearer-token",
        "Submit Notification Template",
        "submit-notification-template",
    )
    .await;
    assert_eq!(StatusCode::CREATED, create_response.status());
    let create_body: Value = create_response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = create_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id");

    let submit_response = client
        .post(format!(
            "{}/api/templates/{}/submit",
            app.address, template_id
        ))
        .bearer_auth("test-bearer-token")
        .send()
        .await
        .expect("Failed to submit template for review");
    assert_eq!(StatusCode::OK, submit_response.status());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let requests = mock_user_service
        .received_requests()
        .await
        .expect("Should capture webhook request");
    let webhook_request = requests
        .iter()
        .find(|request| request.url.path() == "/marketplace/sync")
        .expect("Submit should send marketplace webhook");
    let payload: Value =
        serde_json::from_slice(&webhook_request.body).expect("Webhook body should be valid JSON");

    assert_eq!("template_submitted", payload["action"]);
    assert_eq!(template_id, payload["stack_template_id"]);
    assert_eq!("submit-notification-template", payload["code"]);
}
