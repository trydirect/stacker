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
    client
        .post(format!("{}/api/templates", address))
        .bearer_auth(token)
        .json(&json!({
            "name": name,
            "slug": slug,
        }))
        .send()
        .await
        .expect("Failed to send template create request")
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
