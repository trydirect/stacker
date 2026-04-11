mod common;

use reqwest::{Client, Response, StatusCode};
use serde_json::{json, Value};

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
    assert_eq!(first_id, second_id, "Same-user duplicate slug should upsert");
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
