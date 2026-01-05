mod common;

use chrono::{Duration, Utc};
use reqwest::StatusCode;
use serde_json::json;

fn create_jwt(role: &str, email: &str, expires_in: Duration) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let header = json!({"alg": "HS256", "typ": "JWT"});
    let payload = json!({
        "role": role,
        "email": email,
        "exp": (Utc::now() + expires_in).timestamp(),
    });

    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string());
    let signature = "test_signature"; // Signature not validated in admin_service connector

    format!("{}.{}.{}", header_b64, payload_b64, signature)
}

#[tokio::test]
async fn admin_templates_accepts_valid_jwt() {
    let app = common::spawn_app().await;
    let client = reqwest::Client::new();
    let token = create_jwt("admin_service", "ops@test.com", Duration::minutes(30));

    let response = client
        .get(format!("{}/admin/templates?status=pending", app.address))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(StatusCode::OK, response.status());

    let body = response
        .json::<serde_json::Value>()
        .await
        .expect("Response should be valid JSON");

    assert!(body.get("list").is_some(), "Response should contain template list");
}

#[tokio::test]
async fn admin_templates_rejects_expired_jwt() {
    let app = common::spawn_app().await;
    let client = reqwest::Client::new();
    let token = create_jwt("admin_service", "ops@test.com", Duration::minutes(-5));

    let response = client
        .get(format!("{}/admin/templates?status=pending", app.address))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    let text = response.text().await.expect("Should read body");
    assert!(text.contains("expired"), "Error body should mention expiration: {}", text);
}

#[tokio::test]
async fn admin_templates_requires_admin_role() {
    let app = common::spawn_app().await;
    let client = reqwest::Client::new();
    let token = create_jwt("group_user", "user@test.com", Duration::minutes(10));

    let response = client
        .get(format!("{}/admin/templates?status=pending", app.address))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send request");

    // group_user should not have Casbin rule for admin endpoints -> Forbidden
    assert_eq!(StatusCode::FORBIDDEN, response.status());
}
