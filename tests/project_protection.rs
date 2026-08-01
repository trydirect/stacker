mod common;

use tokio::sync::OnceCell;

static APP: OnceCell<common::TwoUserTestApp> = OnceCell::const_new();

async fn app() -> &'static common::TwoUserTestApp {
    common::get_or_init_two_user_app(&APP)
        .await
        .expect("Failed to start test app")
}

/// Integration tests for project deletion protection.
/// Verifies that protected projects cannot be deleted, protection toggle
/// requires name confirmation to disable, and active resource counts are reported.

#[tokio::test]
async fn test_new_project_default_unprotected() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/project", &app.address))
        .header("Content-Type", "application/json")
        .body(r#"{"custom":{"custom_stack_code":"default-unprot","web":[{"_id":"a1","code":"nginx","name":"Nginx","type":"web","restart":"always","dockerhub_name":"nginx","custom":true}]}}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200, "create project should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["item"]["is_protected"], false,
        "new project should default to is_protected=false"
    );
}

#[tokio::test]
async fn test_enable_protection_via_patch() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": true}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200, "enable protection should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["item"]["is_protected"], true);
}

#[tokio::test]
async fn test_delete_protected_project_returns_403() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // Enable protection
    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": true}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);

    // Try to delete — should be blocked
    let resp = client
        .delete(format!("{}/project/{}", &app.address, project_id))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        403,
        "deleting protected project should return 403"
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["is_protected"], true);
    assert!(
        body["reasons"].is_object(),
        "response should include reasons"
    );
}

#[tokio::test]
async fn test_delete_unprotected_project_succeeds() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    let resp = client
        .delete(format!("{}/project/{}", &app.address, project_id))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        200,
        "deleting unprotected project should succeed"
    );
}

#[tokio::test]
async fn test_disable_protection_requires_name() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // Enable protection
    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": true}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);

    // Try to disable without confirmation_name — should fail
    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": false}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        400,
        "disabling without name should return 400"
    );
}

#[tokio::test]
async fn test_disable_protection_wrong_name_rejected() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // Enable protection
    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": true}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);

    // Try to disable with wrong name
    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": false, "confirmation_name": "wrong-name"}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 400, "wrong name should return 400");
}

#[tokio::test]
async fn test_disable_protection_correct_name_succeeds() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // Enable protection
    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": true}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);

    // Disable with correct name ("Test Project" is the name from create_test_project)
    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": false, "confirmation_name": "Test Project"}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200, "correct name should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["item"]["is_protected"], false);

    // Now delete should succeed
    let resp = client
        .delete(format!("{}/project/{}", &app.address, project_id))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200, "delete after unprotect should succeed");
}

#[tokio::test]
async fn test_protection_shows_deployment_and_server_counts() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;

    // Seed deployments and servers
    for i in 0..2 {
        common::create_test_deployment(
            &app.db_pool,
            common::USER_A_ID,
            project_id,
            &format!("deploy-hash-{}", i),
        )
        .await;
    }
    common::create_test_server(&app.db_pool, common::USER_A_ID, project_id, "active", None).await;

    let client = reqwest::Client::new();

    // Enable protection
    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": true}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);

    // Try to delete — should be blocked with counts
    let resp = client
        .delete(format!("{}/project/{}", &app.address, project_id))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 403);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["reasons"]["active_deployments"], 2);
    assert_eq!(body["reasons"]["active_servers"], 1);
}

#[tokio::test]
async fn test_idor_cannot_toggle_other_users_protection() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // User B tries to enable protection on User A's project
    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": true}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        404,
        "User B must not toggle User A's protection"
    );
}

#[tokio::test]
async fn test_project_list_includes_is_protected() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let client = reqwest::Client::new();

    // Enable protection on the project
    let resp = client
        .patch(format!(
            "{}/project/{}/protection",
            &app.address, project_id
        ))
        .header("Content-Type", "application/json")
        .body(r#"{"is_protected": true}"#)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);

    // List projects — should include is_protected
    let resp = client
        .get(format!("{}/project", &app.address))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    let found = list
        .iter()
        .find(|p| p["id"].as_i64() == Some(project_id as i64));
    assert!(found.is_some(), "project should be in list");
    assert_eq!(found.unwrap()["is_protected"], true);
}
