mod common;

use chrono::{Duration, Utc};
use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::Row;

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

async fn insert_submitted_template(pool: &sqlx::PgPool, creator_user_id: &str, slug: &str) -> String {
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
        VALUES ($1, 'Creator Example', 'Review Template', $2, 'submitted', '[]'::jsonb, '{}'::jsonb)
        RETURNING id"#,
    )
    .bind(creator_user_id)
    .bind(slug)
    .fetch_one(pool)
    .await
    .expect("Failed to insert template")
    .get::<uuid::Uuid, _>("id")
    .to_string()
}

#[tokio::test]
async fn admin_can_mark_template_needs_changes_and_creator_can_see_reason() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_submitted_template(
        &app.db_pool,
        common::USER_A_ID,
        "needs-changes-review-template",
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
