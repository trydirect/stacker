mod common;

use reqwest::StatusCode;
use serde_json::Value;
use sqlx::Row;

const USER_TOKEN: &str = "test-bearer-token";

#[tokio::test]
async fn creator_onboarding_complete_marks_in_progress_profile_completed() {
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
        VALUES ($1, 'pending', 'in_progress', false, 'mock', 'acct_progress', '{"onboarding":{"started_at":"2026-04-12T00:00:00Z","link_request_count":1}}'::jsonb)"#,
    )
    .bind("test_user_id")
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert in-progress vendor profile");

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/templates/mine/vendor-profile/onboarding-complete",
            app.address
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to mark onboarding complete");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("completion response should be valid JSON");

    assert_eq!(true, body["item"]["completion_recorded"]);
    assert_eq!(
        "completed",
        body["item"]["vendor_profile"]["onboarding_status"]
    );
    assert_eq!(false, body["item"]["payout_ready"]);

    let row = sqlx::query(
        r#"SELECT onboarding_status, metadata
           FROM marketplace_vendor_profile WHERE creator_user_id = $1"#,
    )
    .bind("test_user_id")
    .fetch_one(&app.db_pool)
    .await
    .expect("vendor profile row should exist");

    assert_eq!("completed", row.get::<String, _>("onboarding_status"));
    let metadata: Value = row.get("metadata");
    assert_eq!("creator_api", metadata["onboarding"]["completion_source"]);
    assert!(metadata["onboarding"]["completed_at"].is_string());
}

#[tokio::test]
async fn creator_onboarding_complete_is_idempotent_after_completion() {
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
        VALUES ($1, 'pending', 'completed', false, 'mock', 'acct_completed', '{"onboarding":{"completed_at":"2026-04-12T00:00:00Z","completion_source":"creator_api"}}'::jsonb)"#,
    )
    .bind("test_user_id")
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert completed vendor profile");

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/templates/mine/vendor-profile/onboarding-complete",
            app.address
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to mark onboarding complete");

    assert_eq!(StatusCode::OK, response.status());

    let body: Value = response
        .json()
        .await
        .expect("completion response should be valid JSON");

    assert_eq!(false, body["item"]["completion_recorded"]);
    assert_eq!(
        "completed",
        body["item"]["vendor_profile"]["onboarding_status"]
    );
}

#[tokio::test]
async fn creator_onboarding_complete_rejects_missing_linkage() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/templates/mine/vendor-profile/onboarding-complete",
            app.address
        ))
        .bearer_auth(USER_TOKEN)
        .send()
        .await
        .expect("Failed to mark onboarding complete");

    assert_eq!(StatusCode::CONFLICT, response.status());
}

#[tokio::test]
async fn creator_onboarding_complete_requires_authentication() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/templates/mine/vendor-profile/onboarding-complete",
            app.address
        ))
        .send()
        .await
        .expect("Failed to mark onboarding complete");

    assert_eq!(StatusCode::FORBIDDEN, response.status());
}
