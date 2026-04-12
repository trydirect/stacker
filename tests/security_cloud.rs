mod common;

/// IDOR security tests for /cloud endpoints.
/// Verify that User B cannot list, read, update, or delete User A's cloud credentials.

#[tokio::test]
async fn test_list_clouds_only_returns_own() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    // User A creates 2 clouds, User B creates 1
    let _ca1 = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-htz", "htz").await;
    let _ca2 = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-aws", "aws").await;
    let _cb1 =
        common::create_test_cloud(&app.db_pool, common::USER_B_ID, "b-do", "digitalocean").await;

    let client = reqwest::Client::new();

    // User A lists → sees exactly 2
    let resp = client
        .get(&format!("{}/cloud", &app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 2, "User A should see exactly 2 clouds");

    // User B lists → sees exactly 1
    let resp = client
        .get(&format!("{}/cloud", &app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 1, "User B should see exactly 1 cloud");
}

#[tokio::test]
async fn test_get_cloud_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let cloud_id = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-htz", "htz").await;
    let client = reqwest::Client::new();

    // User B tries to GET User A's cloud → 404
    let resp = client
        .get(&format!("{}/cloud/{}", &app.address, cloud_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B must not read User A's cloud"
    );
}

#[tokio::test]
async fn test_update_cloud_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let cloud_id = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-htz", "htz").await;
    let client = reqwest::Client::new();

    // User B tries to PUT User A's cloud → 400 (bad_request = IDOR guard)
    let resp = client
        .put(&format!("{}/cloud/{}", &app.address, cloud_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .header("Content-Type", "application/json")
        .body(r#"{"provider":"htz","cloud_token":"stolen","save_token":true}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "User B must not update User A's cloud"
    );
}

#[tokio::test]
async fn test_delete_cloud_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let cloud_id = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-htz", "htz").await;
    let client = reqwest::Client::new();

    // User B tries to DELETE User A's cloud → 400 (bad_request = IDOR guard)
    let resp = client
        .delete(&format!("{}/cloud/{}", &app.address, cloud_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "User B must not delete User A's cloud"
    );
}

#[tokio::test]
async fn test_owner_can_access_own_cloud() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let cloud_id = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-htz", "htz").await;
    let client = reqwest::Client::new();

    // User A GETs own cloud → 200
    let resp = client
        .get(&format!("{}/cloud/{}", &app.address, cloud_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "Owner must be able to read own cloud"
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["item"].is_object(), "expected item object in response");
}
