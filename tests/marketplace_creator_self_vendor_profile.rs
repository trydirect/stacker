mod common;

use reqwest::StatusCode;
use serde_json::{json, Value};

const USER_TOKEN: &str = "test-bearer-token";

#[tokio::test]
async fn creator_self_vendor_profile_returns_default_profile_when_missing() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let response = reqwest::Client::new()
        .get(format!("{}/api/templates/mine/vendor-profile", app.address))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to fetch self vendor profile");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("self vendor profile response should be valid JSON");

    assert_eq!("test_user_id", body["item"]["creator_user_id"]);
    assert_eq!(false, body["item"]["payout_ready"]);

    let vendor_profile = &body["item"]["vendor_profile"];
    assert_eq!("test_user_id", vendor_profile["creator_user_id"]);
    assert_eq!("unverified", vendor_profile["verification_status"]);
    assert_eq!("not_started", vendor_profile["onboarding_status"]);
    assert_eq!(false, vendor_profile["payouts_enabled"]);
    assert_eq!(Value::Null, vendor_profile["payout_provider"]);
    assert_eq!(json!({}), vendor_profile["metadata"]);
    assert_eq!(None, vendor_profile.get("payout_account_ref"));
}

#[tokio::test]
async fn creator_self_vendor_profile_returns_persisted_profile() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

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
        .get(format!("{}/api/templates/mine/vendor-profile", app.address))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to fetch self vendor profile");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("self vendor profile response should be valid JSON");

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
async fn creator_self_vendor_profile_is_scoped_to_authenticated_user() {
    let app = match common::spawn_app_two_users().await {
        Some(app) => app,
        None => return,
    };

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
    .bind(common::USER_A_ID)
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert vendor profile");

    let response = reqwest::Client::new()
        .get(format!("{}/api/templates/mine/vendor-profile", app.address))
        .bearer_auth(common::USER_B_TOKEN)
        .send()
        .await
        .expect("Failed to fetch self vendor profile");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("self vendor profile response should be valid JSON");

    assert_eq!(common::USER_B_ID, body["item"]["creator_user_id"]);
    assert_eq!(false, body["item"]["payout_ready"]);

    let vendor_profile = &body["item"]["vendor_profile"];
    assert_eq!(common::USER_B_ID, vendor_profile["creator_user_id"]);
    assert_eq!("unverified", vendor_profile["verification_status"]);
    assert_eq!("not_started", vendor_profile["onboarding_status"]);
}

#[tokio::test]
async fn creator_self_vendor_profile_requires_authentication() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let response = reqwest::Client::new()
        .get(format!("{}/api/templates/mine/vendor-profile", app.address))
        .send()
        .await
        .expect("Failed to fetch self vendor profile");

    assert_eq!(StatusCode::FORBIDDEN, response.status());
}
