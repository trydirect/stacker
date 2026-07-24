mod common;

use reqwest::StatusCode;

#[tokio::test]
async fn owner_can_fetch_deployment_plan() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let hash = format!("dpl-{}", uuid::Uuid::new_v4());
    let _deployment_id =
        common::create_test_deployment(&app.db_pool, common::USER_A_ID, project_id, &hash).await;

    let resp = client
        .get(format!("{}/api/v1/deployments/{}/plan", app.address, hash))
        .query(&[("operation", "deploy"), ("target", "cloud")])
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["item"]["schemaVersion"].as_str().unwrap(), "v1alpha1");
    assert_eq!(body["item"]["deploymentHash"].as_str().unwrap(), hash);
    assert_eq!(body["item"]["operation"].as_str().unwrap(), "deploy");
}
