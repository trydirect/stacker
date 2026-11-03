mod common;

use reqwest::{Client, StatusCode};
use serde_json::{json, Value};

#[tokio::test]
async fn create_app_persists_config_contract_and_get_config_returns_it() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;

    let create_response = client
        .post(format!("{}/project/{}/apps", app.address, project_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "code": "auth",
            "image": "supabase/gotrue:latest",
            "env": { "JWT_SECRET": "dev-only-value" },
            "config_contract": {
                "services": {
                    "auth": {
                        "fields": {
                            "JWT_SECRET": { "mutability": "generated", "type": "hex", "length": 32 }
                        }
                    }
                }
            }
        }))
        .send()
        .await
        .expect("Failed to create app");
    assert_eq!(StatusCode::OK, create_response.status());

    let config_response = client
        .get(format!(
            "{}/project/{}/apps/auth/config",
            app.address, project_id
        ))
        .bearer_auth("test-bearer-token")
        .send()
        .await
        .expect("Failed to fetch app config");
    assert_eq!(StatusCode::OK, config_response.status());

    let body: Value = config_response
        .json()
        .await
        .expect("Config response should be valid JSON");
    let config_contract = &body["item"]["config_contract"];
    assert_eq!(
        config_contract["services"]["auth"]["fields"]["JWT_SECRET"]["mutability"],
        "generated"
    );
    assert_eq!(
        config_contract["services"]["auth"]["fields"]["JWT_SECRET"]["type"],
        "hex"
    );
}

#[tokio::test]
async fn get_config_returns_null_config_contract_when_undeclared() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let project_id = common::create_test_project(&app.db_pool, "test_user_id").await;

    let create_response = client
        .post(format!("{}/project/{}/apps", app.address, project_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "code": "web",
            "image": "nginx:stable"
        }))
        .send()
        .await
        .expect("Failed to create app");
    assert_eq!(StatusCode::OK, create_response.status());

    let config_response = client
        .get(format!(
            "{}/project/{}/apps/web/config",
            app.address, project_id
        ))
        .bearer_auth("test-bearer-token")
        .send()
        .await
        .expect("Failed to fetch app config");
    assert_eq!(StatusCode::OK, config_response.status());

    let body: Value = config_response
        .json()
        .await
        .expect("Config response should be valid JSON");
    assert!(body["item"]["config_contract"].is_null());
}
