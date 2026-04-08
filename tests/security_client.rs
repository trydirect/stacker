mod common;

use common::{USER_A_ID, USER_A_TOKEN, USER_B_TOKEN};
use sqlx::Row;

/// User A creates a client. User B tries to update/enable/disable → rejected (400).
/// Verifies cross-user data isolation on client endpoints.

async fn insert_client(pool: &sqlx::PgPool, user_id: &str) -> i32 {
    let rec = sqlx::query(
        "INSERT INTO client (user_id, title, secret, enabled) \
         VALUES ($1, 'test-client', 'secret123', true) RETURNING id",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    rec.get("id")
}

#[tokio::test]
async fn test_list_clients_only_returns_own() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // User A owns a client
    let client_id = insert_client(&app.db_pool, USER_A_ID).await;
    assert!(client_id > 0);

    // User B tries to access User A's client via update (no list endpoint exists).
    // This confirms B cannot interact with A's client at all.
    let resp = client
        .put(format!("{}/client/{}", &app.address, client_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        !resp.status().is_success(),
        "User B should not be able to access User A's client"
    );
}

#[tokio::test]
async fn test_update_client_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let client_id = insert_client(&app.db_pool, USER_A_ID).await;

    let resp = client
        .put(format!("{}/client/{}", &app.address, client_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    // Handler returns 400 Bad Request for non-owner
    assert_eq!(
        resp.status().as_u16(),
        400,
        "User B updating User A's client should return 400"
    );
}

#[tokio::test]
async fn test_enable_client_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // Create a disabled client (secret = NULL) for User A
    let rec = sqlx::query(
        "INSERT INTO client (user_id, secret, enabled) \
         VALUES ($1, NULL, false) RETURNING id",
    )
    .bind(USER_A_ID)
    .fetch_one(&app.db_pool)
    .await
    .unwrap();
    let client_id: i32 = rec.get("id");

    let resp = client
        .put(format!("{}/client/{}/enable", &app.address, client_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        resp.status().as_u16(),
        400,
        "User B enabling User A's client should return 400"
    );
}

#[tokio::test]
async fn test_disable_client_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let client_id = insert_client(&app.db_pool, USER_A_ID).await;

    let resp = client
        .put(format!("{}/client/{}/disable", &app.address, client_id))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        resp.status().as_u16(),
        400,
        "User B disabling User A's client should return 400"
    );
}

#[tokio::test]
async fn test_owner_can_manage_own_client() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let client_id = insert_client(&app.db_pool, USER_A_ID).await;

    // Owner can update (regenerate secret)
    let resp = client
        .put(format!("{}/client/{}", &app.address, client_id))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");
    assert!(
        resp.status().is_success(),
        "Owner should be able to update own client, got {}",
        resp.status()
    );

    // Owner can disable
    let resp = client
        .put(format!("{}/client/{}/disable", &app.address, client_id))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");
    assert!(
        resp.status().is_success(),
        "Owner should be able to disable own client, got {}",
        resp.status()
    );

    // Owner can re-enable
    let resp = client
        .put(format!("{}/client/{}/enable", &app.address, client_id))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");
    assert!(
        resp.status().is_success(),
        "Owner should be able to enable own client, got {}",
        resp.status()
    );
}
