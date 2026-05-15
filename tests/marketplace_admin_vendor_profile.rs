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

async fn insert_template(pool: &sqlx::PgPool, creator_user_id: &str, slug: &str) -> String {
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
        VALUES ($1, 'Vendor Example', 'Vendor Template', $2, 'submitted', '[]'::jsonb, '{}'::jsonb)
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

async fn fetch_template_detail(address: &str, template_id: &str, token: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("{}/api/admin/templates/{}", address, template_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to fetch template detail")
}

#[tokio::test]
async fn admin_detail_returns_default_vendor_profile_when_missing() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        "vendor-user-1",
        "vendor-profile-default-template",
    )
    .await;

    let token = create_admin_jwt();
    let response = fetch_template_detail(&app.address, &template_id, &token).await;

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("detail response should be valid JSON");
    let vendor_profile = &body["item"]["vendor_profile"];

    assert_eq!("vendor-user-1", vendor_profile["creator_user_id"]);
    assert_eq!("unverified", vendor_profile["verification_status"]);
    assert_eq!("not_started", vendor_profile["onboarding_status"]);
    assert_eq!(false, vendor_profile["payouts_enabled"]);
    assert_eq!(Value::Null, vendor_profile["payout_provider"]);
    assert_eq!(Value::Null, vendor_profile["payout_account_ref"]);
    assert_eq!(json!({}), vendor_profile["metadata"]);
    assert_eq!(Value::Null, vendor_profile["created_at"]);
    assert_eq!(Value::Null, vendor_profile["updated_at"]);
}

#[tokio::test]
async fn admin_detail_returns_vendor_profile_for_template_creator() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        "vendor-user-2",
        "vendor-profile-populated-template",
    )
    .await;

    sqlx::query(
        r#"INSERT INTO marketplace_vendor_profile (
            creator_user_id,
            verification_status,
            onboarding_status,
            payouts_enabled,
            payout_provider,
            payout_account_ref,
            metadata
        )
        VALUES ($1, 'verified', 'completed', true, 'stripe_connect', 'acct_123', '{"country":"DE"}'::jsonb)"#,
    )
    .bind("vendor-user-2")
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert vendor profile");

    let token = create_admin_jwt();
    let response = fetch_template_detail(&app.address, &template_id, &token).await;

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("detail response should be valid JSON");
    let vendor_profile = &body["item"]["vendor_profile"];

    assert_eq!("vendor-user-2", vendor_profile["creator_user_id"]);
    assert_eq!("verified", vendor_profile["verification_status"]);
    assert_eq!("completed", vendor_profile["onboarding_status"]);
    assert_eq!(true, vendor_profile["payouts_enabled"]);
    assert_eq!("stripe_connect", vendor_profile["payout_provider"]);
    assert_eq!("acct_123", vendor_profile["payout_account_ref"]);
    assert_eq!(json!({"country":"DE"}), vendor_profile["metadata"]);
    assert!(vendor_profile["created_at"].is_string());
    assert!(vendor_profile["updated_at"].is_string());
}

#[tokio::test]
async fn admin_patch_vendor_profile_creates_profile_when_missing() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let token = create_admin_jwt();
    let template_id = insert_template(
        &app.db_pool,
        "vendor-user-3",
        "vendor-profile-patch-create-template",
    )
    .await;

    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/admin/templates/{}/vendor-profile",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "verification_status": "pending",
            "onboarding_status": "in_progress",
            "payout_provider": "stripe_connect",
            "payout_account_ref": "acct_pending",
            "metadata": {"country": "NL"}
        }))
        .send()
        .await
        .expect("Failed to patch vendor profile");

    assert_eq!(StatusCode::OK, response.status());

    let detail = fetch_template_detail(&app.address, &template_id, &create_admin_jwt()).await;
    assert_eq!(StatusCode::OK, detail.status());

    let body: Value = detail
        .json()
        .await
        .expect("detail response should be valid JSON");
    let vendor_profile = &body["item"]["vendor_profile"];

    assert_eq!("vendor-user-3", vendor_profile["creator_user_id"]);
    assert_eq!("pending", vendor_profile["verification_status"]);
    assert_eq!("in_progress", vendor_profile["onboarding_status"]);
    assert_eq!(false, vendor_profile["payouts_enabled"]);
    assert_eq!("stripe_connect", vendor_profile["payout_provider"]);
    assert_eq!("acct_pending", vendor_profile["payout_account_ref"]);
    assert_eq!(json!({"country":"NL"}), vendor_profile["metadata"]);
    assert!(vendor_profile["created_at"].is_string());
    assert!(vendor_profile["updated_at"].is_string());
}

#[tokio::test]
async fn admin_patch_vendor_profile_preserves_unspecified_fields() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let token = create_admin_jwt();
    let template_id = insert_template(
        &app.db_pool,
        "vendor-user-4",
        "vendor-profile-patch-update-template",
    )
    .await;

    sqlx::query(
        r#"INSERT INTO marketplace_vendor_profile (
            creator_user_id,
            verification_status,
            onboarding_status,
            payouts_enabled,
            payout_provider,
            payout_account_ref,
            metadata
        )
        VALUES ($1, 'verified', 'completed', true, 'stripe_connect', 'acct_live', '{"country":"DE","tier":"gold"}'::jsonb)"#,
    )
    .bind("vendor-user-4")
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert vendor profile");

    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/admin/templates/{}/vendor-profile",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "payouts_enabled": false,
            "metadata": {"country": "FR"}
        }))
        .send()
        .await
        .expect("Failed to patch vendor profile");

    assert_eq!(StatusCode::OK, response.status());

    let detail = fetch_template_detail(&app.address, &template_id, &create_admin_jwt()).await;
    assert_eq!(StatusCode::OK, detail.status());

    let body: Value = detail
        .json()
        .await
        .expect("detail response should be valid JSON");
    let vendor_profile = &body["item"]["vendor_profile"];

    assert_eq!("vendor-user-4", vendor_profile["creator_user_id"]);
    assert_eq!("verified", vendor_profile["verification_status"]);
    assert_eq!("completed", vendor_profile["onboarding_status"]);
    assert_eq!(false, vendor_profile["payouts_enabled"]);
    assert_eq!("stripe_connect", vendor_profile["payout_provider"]);
    assert_eq!("acct_live", vendor_profile["payout_account_ref"]);
    assert_eq!(json!({"country":"FR"}), vendor_profile["metadata"]);
}

#[tokio::test]
async fn admin_patch_vendor_profile_rejects_invalid_status_values() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        "vendor-user-5",
        "vendor-profile-invalid-status-template",
    )
    .await;

    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/admin/templates/{}/vendor-profile",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "verification_status": "not-a-real-status"
        }))
        .send()
        .await
        .expect("Failed to patch vendor profile");

    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

#[tokio::test]
async fn admin_patch_vendor_profile_requires_admin_role() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        "vendor-user-6",
        "vendor-profile-forbidden-template",
    )
    .await;

    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/admin/templates/{}/vendor-profile",
            app.address, template_id
        ))
        .header(
            "Authorization",
            format!("Bearer {}", {
                use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
                let header = json!({"alg": "HS256", "typ": "JWT"});
                let payload = json!({
                    "role": "group_user",
                    "email": "user@test.com",
                    "exp": (Utc::now() + Duration::minutes(30)).timestamp(),
                });
                format!(
                    "{}.{}.{}",
                    URL_SAFE_NO_PAD.encode(header.to_string()),
                    URL_SAFE_NO_PAD.encode(payload.to_string()),
                    "test_signature"
                )
            }),
        )
        .json(&json!({
            "verification_status": "pending"
        }))
        .send()
        .await
        .expect("Failed to patch vendor profile");

    assert_eq!(StatusCode::FORBIDDEN, response.status());
}
