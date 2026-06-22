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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PATCH /mine/vendor-profile — self-service public profile update
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn creator_patch_vendor_public_profile_creates_profile_when_missing() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/templates/mine/vendor-profile",
            app.address
        ))
        .bearer_auth(USER_TOKEN)
        .json(&json!({
            "display_name": "Acme Cloud",
            "bio": "We build stacks.",
            "website_url": "https://acme-cloud.example.com",
            "avatar_url": "https://acme-cloud.example.com/avatar.png",
            "public_slug": "acme-cloud"
        }))
        .send()
        .await
        .expect("Failed to PATCH vendor public profile");

    assert_eq!(StatusCode::OK, response.status());

    // Verify the profile was persisted by reading it back
    let get_response = reqwest::Client::new()
        .get(format!(
            "{}/api/templates/mine/vendor-profile",
            app.address
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to GET self vendor profile after PATCH");

    assert_eq!(StatusCode::OK, get_response.status());

    let body: Value = get_response
        .json()
        .await
        .expect("response should be valid JSON");

    let profile = &body["item"]["vendor_profile"];
    assert_eq!("acme-cloud", profile["public_slug"]);
    assert_eq!("Acme Cloud", profile["display_name"]);
    assert_eq!("We build stacks.", profile["bio"]);
    assert_eq!(
        "https://acme-cloud.example.com/avatar.png",
        profile["avatar_url"]
    );
    assert_eq!(
        "https://acme-cloud.example.com",
        profile["website_url"]
    );
}

#[tokio::test]
async fn creator_patch_vendor_public_profile_updates_existing_profile() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    // Seed an existing public profile
    sqlx::query(
        r#"INSERT INTO marketplace_vendor_profile (
            creator_user_id, public_slug, display_name, bio,
            avatar_url, website_url,
            verification_status, onboarding_status
        )
        VALUES ($1, 'old-slug', 'Old Name', 'Old bio',
                'https://old.example.com/avatar.png', 'https://old.example.com',
                'unverified', 'not_started')"#,
    )
    .bind("test_user_id")
    .execute(&app.db_pool)
    .await
    .expect("Failed to seed vendor profile");

    // Update only display_name and bio — slug and URLs should be preserved via COALESCE
    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/templates/mine/vendor-profile",
            app.address
        ))
        .bearer_auth(USER_TOKEN)
        .json(&json!({
            "display_name": "New Name",
            "bio": "New bio"
        }))
        .send()
        .await
        .expect("Failed to PATCH vendor public profile");

    assert_eq!(StatusCode::OK, response.status());

    // Verify only the specified fields changed
    let get_response = reqwest::Client::new()
        .get(format!(
            "{}/api/templates/mine/vendor-profile",
            app.address
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to GET self vendor profile");

    let body: Value = get_response
        .json()
        .await
        .expect("response should be valid JSON");

    let profile = &body["item"]["vendor_profile"];
    assert_eq!("old-slug", profile["public_slug"]);
    assert_eq!("New Name", profile["display_name"]);
    assert_eq!("New bio", profile["bio"]);
    assert_eq!(
        "https://old.example.com/avatar.png",
        profile["avatar_url"]
    );
    assert_eq!("https://old.example.com", profile["website_url"]);
}

#[tokio::test]
async fn creator_patch_vendor_public_profile_rejects_invalid_slug() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/templates/mine/vendor-profile",
            app.address
        ))
        .bearer_auth(USER_TOKEN)
        .json(&json!({
            "public_slug": "-invalid-slug"
        }))
        .send()
        .await
        .expect("Failed to PATCH vendor public profile with invalid slug");

    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

#[tokio::test]
async fn creator_patch_vendor_public_profile_requires_authentication() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/templates/mine/vendor-profile",
            app.address
        ))
        .json(&json!({
            "display_name": "Should Not Work"
        }))
        .send()
        .await
        .expect("Failed to PATCH vendor public profile without auth");

    assert_eq!(StatusCode::FORBIDDEN, response.status());
}
