mod common;

use reqwest::StatusCode;

#[tokio::test]
async fn project_alias_lists_projects_for_authenticated_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    for _ in 0..2 {
        common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    }

    let resp = client
        .get(format!("{}/api/v1/project", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("json response");
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn project_alias_rejects_unauthenticated_requests() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/v1/project", app.address))
        .send()
        .await
        .expect("request failed");

    assert!(
        resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN,
        "Unauthenticated request to /api/v1/project should be rejected, got {}",
        resp.status()
    );
}
