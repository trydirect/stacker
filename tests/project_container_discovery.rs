mod common;

use reqwest::StatusCode;
use serde_json::json;
use stacker::models::{Agent, Command};

#[tokio::test]
async fn discovery_prefers_active_agent_hash_over_latest_deployment_row() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;

    let active_hash = "deployment-active-hash";
    common::create_test_deployment(&app.db_pool, common::USER_A_ID, project_id, active_hash).await;

    common::create_test_deployment(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        "deployment-latest-hash",
    )
    .await;

    let mut agent = Agent::new(active_hash.to_string());
    agent.mark_online();
    stacker::db::agent::insert(&app.db_pool, agent)
        .await
        .expect("failed to insert active agent");

    let mut list_containers = Command::new(
        "cmd-list-containers".to_string(),
        active_hash.to_string(),
        "list_containers".to_string(),
        common::USER_A_ID.to_string(),
    );
    list_containers.status = "completed".to_string();
    list_containers.result = Some(json!({
        "containers": [
            {
                "name": "project-device-api-1",
                "image": "optimum/syncopia-device-api",
                "status": "running"
            }
        ]
    }));
    stacker::db::command::insert(&app.db_pool, &list_containers)
        .await
        .expect("failed to insert list_containers command");

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/project/{}/containers/discover",
            app.address, project_id
        ))
        .bearer_auth(common::USER_A_TOKEN)
        .send()
        .await
        .expect("request failed");

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("json response");
    let unregistered = body["item"]["unregistered"]
        .as_array()
        .expect("unregistered array");

    assert_eq!(
        unregistered.len(),
        1,
        "expected discover to use active agent hash"
    );
    assert_eq!(unregistered[0]["container_name"], "project-device-api-1");
    assert_eq!(unregistered[0]["suggested_code"], "device-api");
}
