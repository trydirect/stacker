mod common;

use reqwest::StatusCode;
use serde_json::Value;
use sqlx::Row;

const USER_TOKEN: &str = "test-bearer-token";

#[tokio::test]
async fn creator_onboarding_link_creates_profile_when_missing() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/templates/mine/vendor-profile/onboarding-link",
            app.address
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to request onboarding link");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("onboarding link response should be valid JSON");

    assert_eq!("test_user_id", body["item"]["creator_user_id"]);
    assert_eq!(false, body["item"]["payout_ready"]);
    assert_eq!(true, body["item"]["linkage_created"]);

    let vendor_profile = &body["item"]["vendor_profile"];
    assert_eq!("test_user_id", vendor_profile["creator_user_id"]);
    assert_eq!("unverified", vendor_profile["verification_status"]);
    assert_eq!("in_progress", vendor_profile["onboarding_status"]);
    assert_eq!(false, vendor_profile["payouts_enabled"]);
    assert_eq!("mock", vendor_profile["payout_provider"]);
    assert_eq!(None, vendor_profile.get("payout_account_ref"));

    let row = sqlx::query(
        r#"SELECT onboarding_status, payout_provider, payout_account_ref, metadata
           FROM marketplace_vendor_profile WHERE creator_user_id = $1"#,
    )
    .bind("test_user_id")
    .fetch_one(&app.db_pool)
    .await
    .expect("vendor profile row should exist");

    assert_eq!("in_progress", row.get::<String, _>("onboarding_status"));
    assert_eq!("mock", row.get::<String, _>("payout_provider"));
    assert!(!row.get::<String, _>("payout_account_ref").is_empty());
    let metadata: Value = row.get("metadata");
    assert!(metadata["onboarding"]["started_at"].is_string());
    assert!(metadata["onboarding"]["last_link_requested_at"].is_string());
    assert_eq!(1, metadata["onboarding"]["link_request_count"]);
}

#[tokio::test]
async fn creator_onboarding_link_reuses_existing_linkage() {
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
        VALUES ($1, 'pending', 'completed', false, 'mock', 'acct_existing', '{}'::jsonb)"#,
    )
    .bind("test_user_id")
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert vendor profile");

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/templates/mine/vendor-profile/onboarding-link",
            app.address
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to request onboarding link");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("onboarding link response should be valid JSON");

    assert_eq!(false, body["item"]["linkage_created"]);
    assert_eq!(
        "completed",
        body["item"]["vendor_profile"]["onboarding_status"]
    );
    assert_eq!("mock", body["item"]["vendor_profile"]["payout_provider"]);
    assert_eq!(
        None,
        body["item"]["vendor_profile"].get("payout_account_ref")
    );

    let row = sqlx::query(
        r#"SELECT onboarding_status, payout_provider, payout_account_ref, metadata
           FROM marketplace_vendor_profile WHERE creator_user_id = $1"#,
    )
    .bind("test_user_id")
    .fetch_one(&app.db_pool)
    .await
    .expect("vendor profile row should exist");

    assert_eq!("completed", row.get::<String, _>("onboarding_status"));
    assert_eq!("mock", row.get::<String, _>("payout_provider"));
    assert_eq!("acct_existing", row.get::<String, _>("payout_account_ref"));
    let metadata: Value = row.get("metadata");
    assert!(metadata["onboarding"]["last_link_requested_at"].is_string());
    assert_eq!(1, metadata["onboarding"]["link_request_count"]);
}

#[tokio::test]
async fn creator_onboarding_link_is_scoped_to_authenticated_user() {
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
        VALUES ($1, 'verified', 'completed', true, 'mock', 'acct_user_a', '{}'::jsonb)"#,
    )
    .bind(common::USER_A_ID)
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert vendor profile");

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/templates/mine/vendor-profile/onboarding-link",
            app.address
        ))
        .bearer_auth(common::USER_B_TOKEN)
        .send()
        .await
        .expect("Failed to request onboarding link");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("onboarding link response should be valid JSON");

    assert_eq!(common::USER_B_ID, body["item"]["creator_user_id"]);

    let user_a_ref: String = sqlx::query(
        r#"SELECT payout_account_ref FROM marketplace_vendor_profile WHERE creator_user_id = $1"#,
    )
    .bind(common::USER_A_ID)
    .fetch_one(&app.db_pool)
    .await
    .expect("user A profile should still exist")
    .get("payout_account_ref");

    let user_b_ref: String = sqlx::query(
        r#"SELECT payout_account_ref FROM marketplace_vendor_profile WHERE creator_user_id = $1"#,
    )
    .bind(common::USER_B_ID)
    .fetch_one(&app.db_pool)
    .await
    .expect("user B profile should be created")
    .get("payout_account_ref");

    assert_eq!("acct_user_a", user_a_ref);
    assert!(!user_b_ref.is_empty());
    assert_ne!(user_a_ref, user_b_ref);
}

#[tokio::test]
async fn creator_onboarding_link_requires_authentication() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/templates/mine/vendor-profile/onboarding-link",
            app.address
        ))
        .send()
        .await
        .expect("Failed to request onboarding link");

    assert_eq!(StatusCode::FORBIDDEN, response.status());
}
