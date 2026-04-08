mod common;

/// IDOR security tests for /server endpoints.
/// Verify that User B cannot list, read, or delete User A's servers.

#[tokio::test]
async fn test_list_servers_only_returns_own() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    // Each user needs a project (FK constraint: server.project_id → project.id)
    let proj_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let proj_b = common::create_test_project(&app.db_pool, common::USER_B_ID).await;

    // User A: 2 servers, User B: 1 server
    let _sa1 =
        common::create_test_server(&app.db_pool, common::USER_A_ID, proj_a, "none", None).await;
    let _sa2 =
        common::create_test_server(&app.db_pool, common::USER_A_ID, proj_a, "none", None).await;
    let _sb1 =
        common::create_test_server(&app.db_pool, common::USER_B_ID, proj_b, "none", None).await;

    let client = reqwest::Client::new();

    // User A lists → sees exactly 2
    let resp = client
        .get(&format!("{}/server", &app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 2, "User A should see exactly 2 servers");

    // User B lists → sees exactly 1
    let resp = client
        .get(&format!("{}/server", &app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 1, "User B should see exactly 1 server");
}

#[tokio::test]
async fn test_get_server_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let proj_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let server_id =
        common::create_test_server(&app.db_pool, common::USER_A_ID, proj_a, "none", None).await;

    let client = reqwest::Client::new();

    // User B tries to GET User A's server → 404
    let resp = client
        .get(&format!("{}/server/{}", &app.address, server_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B must not read User A's server"
    );
}

#[tokio::test]
async fn test_get_server_by_project_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let proj_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let _server_id =
        common::create_test_server(&app.db_pool, common::USER_A_ID, proj_a, "none", None).await;

    let client = reqwest::Client::new();

    // User B tries to GET servers by User A's project → 404
    let resp = client
        .get(&format!("{}/server/project/{}", &app.address, proj_a))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B must not list servers by User A's project"
    );
}

#[tokio::test]
async fn test_delete_server_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let proj_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let server_id =
        common::create_test_server(&app.db_pool, common::USER_A_ID, proj_a, "none", None).await;

    let client = reqwest::Client::new();

    // User B tries to DELETE User A's server → 400 (bad_request = IDOR guard)
    let resp = client
        .delete(&format!("{}/server/{}", &app.address, server_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "User B must not delete User A's server"
    );
}

#[tokio::test]
async fn test_owner_can_access_own_server() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let proj_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let server_id =
        common::create_test_server(&app.db_pool, common::USER_A_ID, proj_a, "none", None).await;

    let client = reqwest::Client::new();

    // User A GETs own server → 200
    let resp = client
        .get(&format!("{}/server/{}", &app.address, server_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "Owner must be able to read own server"
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["item"].is_object(), "expected item object in response");
}
