mod common;

use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::Row;

const USER_TOKEN: &str = "test-bearer-token";

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

#[tokio::test]
async fn creator_vendor_profile_status_returns_default_profile_when_missing() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        "test_user_id",
        "creator-vendor-profile-status-default",
    )
    .await;

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/templates/{}/vendor-profile-status",
            app.address, template_id
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to fetch vendor profile status");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("status response should be valid JSON");

    assert_eq!(template_id, body["item"]["template_id"]);
    assert_eq!("test_user_id", body["item"]["creator_user_id"]);
    assert_eq!(
        false,
        body["item"]["payout_ready"]
            .as_bool()
            .expect("payout_ready should be bool")
    );

    let vendor_profile = &body["item"]["vendor_profile"];
    assert_eq!("unverified", vendor_profile["verification_status"]);
    assert_eq!("not_started", vendor_profile["onboarding_status"]);
    assert_eq!(false, vendor_profile["payouts_enabled"]);
    assert_eq!(Value::Null, vendor_profile["payout_provider"]);
    assert_eq!(json!({}), vendor_profile["metadata"]);
    assert_eq!(None, vendor_profile.get("payout_account_ref"));
}

#[tokio::test]
async fn creator_vendor_profile_status_returns_persisted_profile() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        "test_user_id",
        "creator-vendor-profile-status-persisted",
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
        VALUES ($1, 'verified', 'completed', true, 'stripe_connect', 'acct_secret', '{"country":"DE"}'::jsonb)"#,
    )
    .bind("test_user_id")
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert vendor profile");

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/templates/{}/vendor-profile-status",
            app.address, template_id
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to fetch vendor profile status");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("status response should be valid JSON");

    assert_eq!(true, body["item"]["payout_ready"]);
    let vendor_profile = &body["item"]["vendor_profile"];
    assert_eq!("verified", vendor_profile["verification_status"]);
    assert_eq!("completed", vendor_profile["onboarding_status"]);
    assert_eq!(true, vendor_profile["payouts_enabled"]);
    assert_eq!("stripe_connect", vendor_profile["payout_provider"]);
    assert_eq!(json!({"country":"DE"}), vendor_profile["metadata"]);
    assert!(vendor_profile["created_at"].is_string());
    assert!(vendor_profile["updated_at"].is_string());
    assert_eq!(None, vendor_profile.get("payout_account_ref"));
}

#[tokio::test]
async fn creator_vendor_profile_status_rejects_non_owner() {
    let app = match common::spawn_app_two_users().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "creator-vendor-profile-status-owner-only",
    )
    .await;

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/templates/{}/vendor-profile-status",
            app.address, template_id
        ))
        .bearer_auth(common::USER_B_TOKEN)
        .send()
        .await
        .expect("Failed to fetch vendor profile status");

    assert_eq!(StatusCode::FORBIDDEN, response.status());
}

#[tokio::test]
async fn creator_vendor_profile_status_requires_authentication() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        "test_user_id",
        "creator-vendor-profile-status-auth-required",
    )
    .await;

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/templates/{}/vendor-profile-status",
            app.address, template_id
        ))
        .send()
        .await
        .expect("Failed to fetch vendor profile status");

    assert_eq!(StatusCode::FORBIDDEN, response.status());
}

#[tokio::test]
async fn creator_vendor_profile_status_returns_not_found_for_unknown_template() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/templates/{}/vendor-profile-status",
            app.address,
            uuid::Uuid::new_v4()
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to fetch vendor profile status");

    assert_eq!(StatusCode::NOT_FOUND, response.status());
}
