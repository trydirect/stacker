/// Security tests for CLI-facing endpoints.
///
/// These tests verify that every API endpoint the `stacker` CLI calls
/// is properly scoped to the authenticated user. They exercise the
/// same HTTP paths that `stacker list projects`, `stacker list clouds`,
/// `stacker list servers`, `stacker list deployments`, `stacker deploy`,
/// and `stacker destroy` hit.
///
/// Each test uses `spawn_app_two_users()` — User A is the owner,
/// User B is the attacker who must be rejected or isolated.
mod common;

use reqwest::StatusCode;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

async fn seed_full_deployment(pool: &sqlx::PgPool, user_id: &str) -> (i32, i32, String) {
    let project_id = common::create_test_project(pool, user_id).await;
    let hash = format!("dpl-{}", uuid::Uuid::new_v4());
    let deployment_id = common::create_test_deployment(pool, user_id, project_id, &hash).await;
    (project_id, deployment_id, hash)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker list projects — GET /project
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_list_projects_user_isolation() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // Seed: User A gets 3, User B gets 1
    for _ in 0..3 {
        common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    }
    common::create_test_project(&app.db_pool, common::USER_B_ID).await;

    // User A sees exactly 3
    let resp = client
        .get(format!("{}/project", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(
        list.len(),
        3,
        "User A should see exactly 3 projects, got {}",
        list.len()
    );

    // User B sees exactly 1
    let resp = client
        .get(format!("{}/project", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(
        list.len(),
        1,
        "User B should see exactly 1 project, got {}",
        list.len()
    );
}

#[tokio::test]
async fn test_cli_list_projects_unauthenticated() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/project", app.address))
        .send()
        .await
        .expect("request failed");

    // Should reject without auth
    assert!(
        resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN,
        "Unauthenticated request to /project should be rejected, got {}",
        resp.status()
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker list clouds — GET /cloud
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_list_clouds_user_isolation() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // Seed: User A has 2 cloud creds, User B has 1
    common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-htz-1", "htz").await;
    common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-htz-2", "htz").await;
    common::create_test_cloud(&app.db_pool, common::USER_B_ID, "b-aws", "aws").await;

    // User A sees 2
    let resp = client
        .get(format!("{}/cloud", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 2, "User A should see 2 clouds");

    // User B sees 1
    let resp = client
        .get(format!("{}/cloud", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 1, "User B should see 1 cloud");
}

#[tokio::test]
async fn test_cli_get_cloud_cross_user_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let cloud_id = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-htz", "htz").await;

    // User B tries to read User A's cloud by ID
    let resp = client
        .get(format!("{}/cloud/{}", app.address, cloud_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not access User A's cloud credentials"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker list servers — GET /server
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_list_servers_user_isolation() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // Servers need a project FK
    let proj_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let proj_b = common::create_test_project(&app.db_pool, common::USER_B_ID).await;

    common::create_test_server(&app.db_pool, common::USER_A_ID, proj_a, "ready", None).await;
    common::create_test_server(&app.db_pool, common::USER_A_ID, proj_a, "ready", None).await;
    common::create_test_server(&app.db_pool, common::USER_B_ID, proj_b, "ready", None).await;

    // User A sees 2
    let resp = client
        .get(format!("{}/server", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 2, "User A should see 2 servers");

    // User B sees 1
    let resp = client
        .get(format!("{}/server", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 1, "User B should see 1 server");
}

#[tokio::test]
async fn test_cli_get_server_cross_user_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let proj_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let server_id =
        common::create_test_server(&app.db_pool, common::USER_A_ID, proj_a, "ready", None).await;

    let resp = client
        .get(format!("{}/server/{}", app.address, server_id))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not access User A's server"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker list deployments — GET /api/v1/deployments
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_list_deployments_user_isolation() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (_, _, hash_a) = seed_full_deployment(&app.db_pool, common::USER_A_ID).await;
    let (_, _, hash_b) = seed_full_deployment(&app.db_pool, common::USER_B_ID).await;

    // User A sees only their own
    let resp = client
        .get(format!("{}/api/v1/deployments", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 1, "User A should see exactly 1 deployment");
    assert_eq!(list[0]["deployment_hash"].as_str().unwrap(), hash_a);

    // User B sees only their own
    let resp = client
        .get(format!("{}/api/v1/deployments", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["list"].as_array().expect("expected list");
    assert_eq!(list.len(), 1, "User B should see exactly 1 deployment");
    assert_eq!(list[0]["deployment_hash"].as_str().unwrap(), hash_b);
}

#[tokio::test]
async fn test_cli_get_deployment_by_hash_cross_user_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (_, _, hash_a) = seed_full_deployment(&app.db_pool, common::USER_A_ID).await;

    // User B tries to fetch User A's deployment by hash
    let resp = client
        .get(format!(
            "{}/api/v1/deployments/hash/{}",
            app.address, hash_a
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not access User A's deployment by hash"
    );
}

#[tokio::test]
async fn test_cli_get_deployment_by_id_cross_user_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (_, did_a, _) = seed_full_deployment(&app.db_pool, common::USER_A_ID).await;

    let resp = client
        .get(format!("{}/api/v1/deployments/{}", app.address, did_a))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not access User A's deployment by ID"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker deploy — POST /project/{id}/deploy[/{cloud_id}]
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_deploy_cross_user_project_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let project_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;

    // User B tries to deploy User A's project
    let deploy_body = serde_json::json!({
        "body": "{}",
        "docker_compose": "version: '3'\nservices:\n  web:\n    image: nginx",
    });

    let resp = client
        .post(format!("{}/project/{}/deploy", app.address, project_a))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&deploy_body)
        .send()
        .await
        .unwrap();

    // Must be rejected — 403 or 404
    assert!(
        resp.status() == StatusCode::NOT_FOUND
            || resp.status() == StatusCode::FORBIDDEN
            || resp.status() == StatusCode::BAD_REQUEST,
        "User B must not deploy User A's project (got {})",
        resp.status()
    );
}

#[tokio::test]
async fn test_cli_deploy_with_cross_user_cloud_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // User B's project + User A's cloud credentials
    let proj_b = common::create_test_project(&app.db_pool, common::USER_B_ID).await;
    let cloud_a = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-htz", "htz").await;

    let deploy_body = serde_json::json!({
        "body": "{}",
        "docker_compose": "version: '3'\nservices:\n  web:\n    image: nginx",
    });

    // User B tries to deploy their project using User A's cloud creds
    let resp = client
        .post(format!(
            "{}/project/{}/deploy/{}",
            app.address, proj_b, cloud_a
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&deploy_body)
        .send()
        .await
        .unwrap();

    // Must be rejected
    assert!(
        resp.status() == StatusCode::NOT_FOUND
            || resp.status() == StatusCode::FORBIDDEN
            || resp.status() == StatusCode::BAD_REQUEST,
        "User B must not use User A's cloud credentials for deploy (got {})",
        resp.status()
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker destroy — POST /api/v1/deployments/{id}/force-complete
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_destroy_cross_user_deployment_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (_, did_a, _) = seed_full_deployment(&app.db_pool, common::USER_A_ID).await;

    // User B tries to force-complete (destroy) User A's deployment
    let resp = client
        .post(format!(
            "{}/api/v1/deployments/{}/force-complete?force=true",
            app.address, did_a
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not destroy User A's deployment"
    );
}

#[tokio::test]
async fn test_cli_destroy_own_deployment_allowed() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (_, did_a, _) = seed_full_deployment(&app.db_pool, common::USER_A_ID).await;

    // User A can force-complete their own deployment
    let resp = client
        .post(format!(
            "{}/api/v1/deployments/{}/force-complete?force=true",
            app.address, did_a
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .unwrap();

    assert!(
        resp.status().is_success(),
        "Owner should be able to destroy their own deployment (got {})",
        resp.status()
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker deploy — enqueue agent command on other user's deployment
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_enqueue_command_cross_user_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let (_, _, hash_a) = seed_full_deployment(&app.db_pool, common::USER_A_ID).await;

    let cmd = serde_json::json!({
        "deployment_hash": hash_a,
        "command_type": "health_check",
        "parameters": {},
    });

    // User B tries to enqueue a command on User A's deployment
    let resp = client
        .post(format!("{}/api/v1/agent/commands/enqueue", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&cmd)
        .send()
        .await
        .unwrap();

    assert!(
        resp.status() == StatusCode::NOT_FOUND
            || resp.status() == StatusCode::FORBIDDEN
            || resp.status() == StatusCode::BAD_REQUEST,
        "User B must not enqueue commands on User A's deployment (got {})",
        resp.status()
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker delete project — DELETE /project/{id}
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_delete_project_cross_user_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let proj_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;

    let resp = client
        .delete(format!("{}/project/{}", app.address, proj_a))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not delete User A's project"
    );

    // Verify project still exists — User A can still fetch it
    let resp = client
        .get(format!("{}/project/{}", app.address, proj_a))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "Project should still exist after cross-user delete attempt"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker delete cloud — DELETE /cloud/{id}
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_delete_cloud_cross_user_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let cloud_a = common::create_test_cloud(&app.db_pool, common::USER_A_ID, "a-htz", "htz").await;

    let resp = client
        .delete(format!("{}/cloud/{}", app.address, cloud_a))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not delete User A's cloud credentials"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker delete server — DELETE /server/{id}
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_delete_server_cross_user_rejected() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let proj_a = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let server_a =
        common::create_test_server(&app.db_pool, common::USER_A_ID, proj_a, "ready", None).await;

    let resp = client
        .delete(format!("{}/server/{}", app.address, server_a))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "User B must not delete User A's server"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Unauthenticated access denied on all CLI endpoints
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[tokio::test]
async fn test_cli_endpoints_reject_unauthenticated() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let endpoints = vec![
        ("GET", format!("{}/project", app.address)),
        ("GET", format!("{}/cloud", app.address)),
        ("GET", format!("{}/server", app.address)),
        ("GET", format!("{}/api/v1/deployments", app.address)),
    ];

    for (method, url) in endpoints {
        let resp = match method {
            "GET" => client.get(&url).send().await.unwrap(),
            _ => unreachable!(),
        };

        let status = resp.status();
        assert!(
            status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
            "{} {} should reject unauthenticated (got {})",
            method,
            url,
            status
        );
    }
}
