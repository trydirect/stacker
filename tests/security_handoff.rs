//! Security and lifecycle tests for CLI handoff endpoints.

mod common;

use common::{
    create_test_deployment, create_test_project, spawn_app_two_users, USER_A_ID, USER_A_TOKEN,
    USER_B_TOKEN,
};

#[tokio::test]
async fn test_owner_can_mint_and_resolve_handoff() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let project_id = create_test_project(&app.db_pool, USER_A_ID).await;
    let deployment_id =
        create_test_deployment(&app.db_pool, USER_A_ID, project_id, "handoff-dep-001").await;

    let mint = client
        .post(format!("{}/api/v1/handoff/mint", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .json(&serde_json::json!({
            "deployment_id": deployment_id,
            "deployment_hash": "handoff-dep-001"
        }))
        .send()
        .await
        .expect("Failed to mint handoff");

    assert_eq!(mint.status(), 200, "Mint should succeed for owner");
    let minted: serde_json::Value = mint.json().await.expect("Mint response must be json");
    let token = minted["item"]["token"]
        .as_str()
        .expect("Mint response should include handoff token");
    let command = minted["item"]["command"]
        .as_str()
        .expect("Mint response should include CLI command");
    assert!(command.contains("stacker connect"));
    assert!(command.contains(token));

    let resolve = client
        .post(format!("{}/api/v1/handoff/resolve", &app.address))
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
        .expect("Failed to resolve handoff");

    assert_eq!(resolve.status(), 200, "Resolve should succeed");
    let resolved: serde_json::Value = resolve.json().await.expect("Resolve response must be json");
    assert_eq!(resolved["item"]["deployment"]["id"], deployment_id);
    assert_eq!(resolved["item"]["deployment"]["hash"], "handoff-dep-001");
    assert_eq!(resolved["item"]["project"]["id"], project_id);
    assert_eq!(
        resolved["item"]["credentials"]["access_token"],
        USER_A_TOKEN
    );
}

#[tokio::test]
async fn test_handoff_resolve_is_single_use() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let project_id = create_test_project(&app.db_pool, USER_A_ID).await;
    let deployment_id =
        create_test_deployment(&app.db_pool, USER_A_ID, project_id, "handoff-dep-once").await;

    let mint = client
        .post(format!("{}/api/v1/handoff/mint", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .json(&serde_json::json!({
            "deployment_id": deployment_id
        }))
        .send()
        .await
        .expect("Failed to mint handoff");
    let minted: serde_json::Value = mint.json().await.expect("Mint response must be json");
    let token = minted["item"]["token"]
        .as_str()
        .expect("Mint response should include handoff token");

    let first = client
        .post(format!("{}/api/v1/handoff/resolve", &app.address))
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
        .expect("Failed to resolve handoff the first time");
    assert_eq!(first.status(), 200);

    let second = client
        .post(format!("{}/api/v1/handoff/resolve", &app.address))
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
        .expect("Failed to resolve handoff the second time");
    assert_eq!(second.status(), 404, "Resolved token must not be reusable");
}

#[tokio::test]
async fn test_handoff_mint_rejects_other_user() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let project_id = create_test_project(&app.db_pool, USER_A_ID).await;
    let deployment_id =
        create_test_deployment(&app.db_pool, USER_A_ID, project_id, "handoff-dep-owner").await;

    let resp = client
        .post(format!("{}/api/v1/handoff/mint", &app.address))
        .header("Authorization", format!("Bearer {}", USER_B_TOKEN))
        .json(&serde_json::json!({
            "deployment_id": deployment_id,
            "deployment_hash": "handoff-dep-owner"
        }))
        .send()
        .await
        .expect("Failed to send mint request");

    assert!(
        resp.status() == 403 || resp.status() == 404,
        "Other user should not mint handoff for another user's deployment. Got: {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_handoff_mint_rejects_unauthenticated() {
    let Some(app) = spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/v1/handoff/mint", &app.address))
        .json(&serde_json::json!({ "deployment_hash": "handoff-no-auth" }))
        .send()
        .await
        .expect("Failed to send unauthenticated mint request");

    assert_eq!(resp.status(), 403);
}
