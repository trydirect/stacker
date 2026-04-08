mod common;

/// IDOR security tests for /project endpoints.
/// Verify that User B cannot list, read, update, or delete User A's projects.

#[tokio::test]
async fn test_list_projects_only_returns_own() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    // User A creates 2 projects, User B creates 1
    let _pa1 = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let _pa2 = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let _pb1 = common::create_test_project(&app.db_pool, common::USER_B_ID).await;

    let client = reqwest::Client::new();

    // User A lists → sees exactly 2
    let resp = client
        .get(&format!("{}/project", &app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 2, "User A should see exactly 2 projects");

    // User B lists → sees exactly 1
    let resp = client
        .get(&format!("{}/project", &app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 1, "User B should see exactly 1 project");
}

#[tokio::test]
async fn test_get_project_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // User B tries to GET User A's project → 404
    let resp = client
        .get(&format!("{}/project/{}", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "User B must not read User A's project"
    );
}

#[tokio::test]
async fn test_update_project_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // User B tries to PUT User A's project → 400 (bad_request = IDOR guard)
    let resp = client
        .put(&format!("{}/project/{}", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .header("Content-Type", "application/json")
        .body(r#"{"custom_stack_code":"hijacked","commonDomain":"test.com","dockerhub_user":"x","dockerhub_password":"x","apps":[]}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "User B must not update User A's project"
    );
}

#[tokio::test]
async fn test_delete_project_rejects_other_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // User B tries to DELETE User A's project → 400 (bad_request = IDOR guard)
    let resp = client
        .delete(&format!("{}/project/{}", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "User B must not delete User A's project"
    );
}

#[tokio::test]
async fn test_owner_can_access_own_project() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // User A GETs own project → 200
    let resp = client
        .get(&format!("{}/project/{}", &app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "Owner must be able to read own project"
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["item"].is_object(), "expected item object in response");
}
