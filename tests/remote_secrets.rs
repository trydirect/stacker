mod common;

use actix_web::web;
use reqwest::StatusCode;
use serde_json::{json, Value};
use stacker::configuration::get_configuration;
use stacker::db;
use stacker::mcp::protocol::ToolContent;
use stacker::mcp::tools::{
    GetAppConfigTool, GetAppEnvVarsTool, GetRemoteServiceSecretTool, SetAppEnvVarTool,
};
use stacker::mcp::{ToolContext, ToolHandler};
use stacker::models::ProjectApp;
use stacker::services::{runtime_env_contract_response, ConfigRenderer, VaultService};
use std::sync::Arc;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct TwoUserVaultApp {
    app: common::TwoUserTestApp,
    vault_server: MockServer,
}

async fn spawn_two_user_app_with_vault() -> Option<TwoUserVaultApp> {
    let mut configuration = get_configuration().expect("Failed to get configuration");
    let vault_server = MockServer::start().await;

    configuration.vault.address = vault_server.uri();
    configuration.vault.token = "test-vault-token".to_string();
    configuration.vault.api_prefix = "v1".to_string();
    configuration.vault.ssh_key_path_prefix = Some("users".to_string());
    configuration.connectors.install_service =
        Some(stacker::connectors::InstallServiceConfig { enabled: false });

    let app = common::spawn_app_two_users_with_configuration(configuration).await?;

    Some(TwoUserVaultApp { app, vault_server })
}

async fn create_test_project_app(pool: &sqlx::PgPool, project_id: i32, code: &str) -> ProjectApp {
    let app = ProjectApp::new(
        project_id,
        code.to_string(),
        "Test App".to_string(),
        "nginx:stable".to_string(),
    );

    db::project_app::insert(pool, &app)
        .await
        .expect("Failed to insert test app")
}

fn service_secret_path_regex(user_id: &str, project_id: i32, app_code: &str, name: &str) -> String {
    format!(
        r"/v1/agent/users/{}/projects/{}/apps/{}/secrets/{}",
        user_id, project_id, app_code, name
    )
}

fn server_secret_path_regex(user_id: &str, server_id: i32, name: &str) -> String {
    format!(
        r"/v1/agent/users/{}/servers/{}/secrets/{}",
        user_id, server_id, name
    )
}

fn tool_context(pool: &sqlx::PgPool) -> ToolContext {
    ToolContext {
        user: Arc::new(stacker::models::User {
            id: common::USER_A_ID.to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            email: common::USER_A_EMAIL.to_string(),
            role: "group_user".to_string(),
            email_confirmed: true,
            mfa_verified: true,
            access_token: None,
        }),
        pg_pool: pool.clone(),
        settings: web::Data::new(stacker::configuration::Settings::default()),
    }
}

fn tool_text_json(content: ToolContent) -> Value {
    match content {
        ToolContent::Text { text } => {
            serde_json::from_str(&text).expect("tool response should be valid json")
        }
        ToolContent::Image { .. } => panic!("expected text tool response"),
    }
}

#[tokio::test]
async fn test_service_secret_crud_returns_metadata_only_and_uses_vault_v1() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let project_app = create_test_project_app(&app.db_pool, project_id, "web").await;
    let secret_name = "S3_KEY";
    let secret_path = service_secret_path_regex(
        common::USER_A_ID,
        project_id,
        &project_app.code,
        secret_name,
    );

    Mock::given(method("POST"))
        .and(path_regex(secret_path.clone()))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.vault_server)
        .await;

    Mock::given(method("DELETE"))
        .and(path_regex(secret_path.clone()))
        .respond_with(ResponseTemplate::new(204))
        .mount(&app.vault_server)
        .await;

    let client = reqwest::Client::new();
    let put_response = client
        .put(format!(
            "{}/project/{}/apps/{}/secrets/{}",
            app.address, project_id, project_app.code, secret_name
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "value": "supersecret" }))
        .send()
        .await
        .expect("service secret PUT failed");

    assert_eq!(put_response.status(), StatusCode::OK);
    let put_body: Value = put_response.json().await.unwrap();
    assert_eq!(put_body["item"]["name"], secret_name);
    assert_eq!(put_body["item"]["scope"], "service");
    assert_eq!(put_body["item"]["app_code"], project_app.code);
    assert_eq!(put_body["item"]["source"], "vault");
    assert_eq!(put_body["item"]["secure"], true);
    assert!(put_body["item"].get("value").is_none());

    let get_response = client
        .get(format!(
            "{}/project/{}/apps/{}/secrets/{}",
            app.address, project_id, project_app.code, secret_name
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("service secret GET failed");

    assert_eq!(get_response.status(), StatusCode::OK);
    let get_body: Value = get_response.json().await.unwrap();
    assert_eq!(get_body["item"]["name"], secret_name);
    assert_eq!(get_body["item"]["secure"], true);
    assert!(get_body["item"].get("value").is_none());

    let list_response = client
        .get(format!(
            "{}/project/{}/apps/{}/secrets",
            app.address, project_id, project_app.code
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("service secret LIST failed");

    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body: Value = list_response.json().await.unwrap();
    assert_eq!(list_body["list"].as_array().unwrap().len(), 1);
    assert_eq!(list_body["list"][0]["name"], secret_name);
    assert_eq!(list_body["list"][0]["secure"], true);
    assert!(list_body["list"][0].get("value").is_none());

    let delete_response = client
        .delete(format!(
            "{}/project/{}/apps/{}/secrets/{}",
            app.address, project_id, project_app.code, secret_name
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("service secret DELETE failed");

    assert_eq!(delete_response.status(), StatusCode::OK);

    let list_after_delete = client
        .get(format!(
            "{}/project/{}/apps/{}/secrets",
            app.address, project_id, project_app.code
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("service secret LIST-after-delete failed");

    assert_eq!(list_after_delete.status(), StatusCode::OK);
    let list_after_delete_body: Value = list_after_delete.json().await.unwrap();
    assert_eq!(list_after_delete_body["list"].as_array().unwrap().len(), 0);

    let requests = app.vault_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method.to_string(), "POST");
    assert_eq!(requests[1].method.to_string(), "DELETE");
    assert!(requests
        .iter()
        .all(|request| !request.url.path().contains("/data/")));
    assert!(requests
        .iter()
        .all(|request| !request.url.path().contains("/metadata/")));
}

#[tokio::test]
async fn test_server_secret_crud_returns_metadata_only_and_uses_vault_v1() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let server_id =
        common::create_test_server(&app.db_pool, common::USER_A_ID, project_id, "none", None).await;
    let secret_name = "HOST_TOKEN";
    let secret_path = server_secret_path_regex(common::USER_A_ID, server_id, secret_name);

    Mock::given(method("POST"))
        .and(path_regex(secret_path.clone()))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.vault_server)
        .await;

    Mock::given(method("DELETE"))
        .and(path_regex(secret_path.clone()))
        .respond_with(ResponseTemplate::new(204))
        .mount(&app.vault_server)
        .await;

    let client = reqwest::Client::new();
    let put_response = client
        .put(format!(
            "{}/server/{}/secrets/{}",
            app.address, server_id, secret_name
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "value": "serversecret" }))
        .send()
        .await
        .expect("server secret PUT failed");

    assert_eq!(put_response.status(), StatusCode::OK);
    let put_body: Value = put_response.json().await.unwrap();
    assert_eq!(put_body["item"]["name"], secret_name);
    assert_eq!(put_body["item"]["scope"], "server");
    assert_eq!(put_body["item"]["server_id"], server_id);
    assert_eq!(put_body["item"]["source"], "vault");
    assert!(put_body["item"].get("value").is_none());

    let get_response = client
        .get(format!(
            "{}/server/{}/secrets/{}",
            app.address, server_id, secret_name
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("server secret GET failed");

    assert_eq!(get_response.status(), StatusCode::OK);
    let get_body: Value = get_response.json().await.unwrap();
    assert_eq!(get_body["item"]["name"], secret_name);
    assert!(get_body["item"].get("value").is_none());

    let list_response = client
        .get(format!("{}/server/{}/secrets", app.address, server_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("server secret LIST failed");

    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body: Value = list_response.json().await.unwrap();
    assert_eq!(list_body["list"].as_array().unwrap().len(), 1);
    assert_eq!(list_body["list"][0]["name"], secret_name);
    assert!(list_body["list"][0].get("value").is_none());

    let delete_response = client
        .delete(format!(
            "{}/server/{}/secrets/{}",
            app.address, server_id, secret_name
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("server secret DELETE failed");

    assert_eq!(delete_response.status(), StatusCode::OK);

    let list_after_delete = client
        .get(format!("{}/server/{}/secrets", app.address, server_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("server secret LIST-after-delete failed");

    assert_eq!(list_after_delete.status(), StatusCode::OK);
    let list_after_delete_body: Value = list_after_delete.json().await.unwrap();
    assert_eq!(list_after_delete_body["list"].as_array().unwrap().len(), 0);

    let requests = app.vault_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method.to_string(), "POST");
    assert_eq!(requests[1].method.to_string(), "DELETE");
    assert!(requests
        .iter()
        .all(|request| !request.url.path().contains("/data/")));
    assert!(requests
        .iter()
        .all(|request| !request.url.path().contains("/metadata/")));
}

#[tokio::test]
async fn test_service_secret_idor_returns_404_without_touching_vault() {
    let Some(app) = spawn_two_user_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.app.db_pool, common::USER_A_ID).await;
    let project_app = create_test_project_app(&app.app.db_pool, project_id, "web").await;

    let client = reqwest::Client::new();
    let response = client
        .put(format!(
            "{}/project/{}/apps/{}/secrets/S3_SECRET",
            app.app.address, project_id, project_app.code
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&json!({ "value": "attacker-secret" }))
        .send()
        .await
        .expect("service secret IDOR request failed");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(app.vault_server.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn test_server_secret_idor_returns_404_without_touching_vault() {
    let Some(app) = spawn_two_user_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.app.db_pool, common::USER_A_ID).await;
    let server_id = common::create_test_server(
        &app.app.db_pool,
        common::USER_A_ID,
        project_id,
        "none",
        None,
    )
    .await;

    let client = reqwest::Client::new();
    let response = client
        .put(format!(
            "{}/server/{}/secrets/HOST_TOKEN",
            app.app.address, server_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_B_TOKEN))
        .json(&json!({ "value": "attacker-secret" }))
        .send()
        .await
        .expect("server secret IDOR request failed");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(app.vault_server.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn test_render_bundle_merges_service_secrets_and_overrides_plain_env() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let mut project = db::project::fetch(&app.db_pool, project_id)
        .await
        .expect("project fetch failed")
        .expect("project missing");
    project.request_json = json!({
        "report": {
            "deployment_hash": "deploy-hash-123"
        }
    });

    let mut project_app = create_test_project_app(&app.db_pool, project_id, "web").await;
    project_app.environment = Some(json!({
        "PLAIN_ONLY": "db-value",
        "S3_KEY": "db-overridden"
    }));
    project_app = db::project_app::update(&app.db_pool, &project_app)
        .await
        .expect("project app update failed");

    let vault_path = format!(
        "agent/users/{}/projects/{}/apps/{}/secrets/S3_KEY",
        common::USER_A_ID,
        project_id,
        project_app.code
    );
    db::remote_secret::upsert_service_secret(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        &project_app.code,
        "S3_KEY",
        &vault_path,
        common::USER_A_ID,
        "synced",
    )
    .await
    .expect("service secret metadata insert failed");

    Mock::given(method("GET"))
        .and(path_regex(format!(r"/v1/{}", vault_path)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "data": {
                    "value": "vault-wins"
                }
            }
        })))
        .mount(&app.vault_server)
        .await;

    let mut configuration = get_configuration().expect("Failed to get configuration");
    configuration.vault.address = app.vault_server.uri();
    configuration.vault.token = "test-vault-token".to_string();
    configuration.vault.api_prefix = "v1".to_string();
    configuration.vault.agent_path_prefix = "agent".to_string();
    let vault_service =
        VaultService::from_settings(&configuration.vault).expect("failed to build vault service");
    let renderer = ConfigRenderer::with_vault(vault_service).expect("renderer init failed");

    let bundle = renderer
        .render_bundle(
            &app.db_pool,
            &project,
            &[project_app.clone()],
            "deploy-hash-123",
        )
        .await
        .expect("render bundle failed");

    let env_content = &bundle
        .app_configs
        .get(&project_app.code)
        .expect("missing env config")
        .content;

    assert!(env_content.contains("PLAIN_ONLY=db-value"));
    assert!(env_content.contains("S3_KEY=vault-wins"));
    assert!(!env_content.contains("S3_KEY=db-overridden"));
    assert!(bundle.compose_content.contains("S3_KEY=vault-wins"));
}

#[tokio::test]
async fn test_scn_003_service_secret_injects_only_into_registered_service_target() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let mut project = db::project::fetch(&app.db_pool, project_id)
        .await
        .expect("project fetch failed")
        .expect("project missing");
    project.request_json = json!({
        "report": {
            "deployment_hash": "deploy-hash-upload"
        }
    });

    let device_api = create_test_project_app(&app.db_pool, project_id, "device-api").await;
    let mut upload = create_test_project_app(&app.db_pool, project_id, "upload").await;
    upload.environment = Some(json!({
        "S3_BUCKET": "plain-value"
    }));
    let upload = db::project_app::update(&app.db_pool, &upload)
        .await
        .expect("upload app update failed");

    let vault_path = format!(
        "agent/users/{}/projects/{}/apps/{}/secrets/S3_BUCKET",
        common::USER_A_ID,
        project_id,
        upload.code
    );
    db::remote_secret::upsert_service_secret(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        &upload.code,
        "S3_BUCKET",
        &vault_path,
        common::USER_A_ID,
        "synced",
    )
    .await
    .expect("service secret metadata insert failed");

    Mock::given(method("GET"))
        .and(path_regex(format!(r"/v1/{}", vault_path)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "data": {
                    "value": "superbucket"
                }
            }
        })))
        .mount(&app.vault_server)
        .await;

    let mut configuration = get_configuration().expect("Failed to get configuration");
    configuration.vault.address = app.vault_server.uri();
    configuration.vault.token = "test-vault-token".to_string();
    configuration.vault.api_prefix = "v1".to_string();
    configuration.vault.agent_path_prefix = "agent".to_string();
    let vault_service =
        VaultService::from_settings(&configuration.vault).expect("failed to build vault service");
    let renderer = ConfigRenderer::with_vault(vault_service).expect("renderer init failed");

    let bundle = renderer
        .render_bundle(
            &app.db_pool,
            &project,
            &[device_api.clone(), upload.clone()],
            "deploy-hash-upload",
        )
        .await
        .expect("render bundle failed");

    let upload_env = &bundle.app_configs.get("upload").unwrap().content;
    let device_api_env = &bundle.app_configs.get("device-api").unwrap().content;

    assert!(upload_env.contains("S3_BUCKET=superbucket"));
    assert!(!upload_env.contains("S3_BUCKET=plain-value"));
    assert!(!device_api_env.contains("S3_BUCKET="));
}

#[tokio::test]
async fn test_get_env_vars_includes_remote_secret_placeholders() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let mut project_app = create_test_project_app(&app.db_pool, project_id, "web").await;
    project_app.environment = Some(json!({
        "VISIBLE_KEY": "plain-value"
    }));
    db::project_app::update(&app.db_pool, &project_app)
        .await
        .expect("project app update failed");

    Mock::given(method("POST"))
        .and(path_regex(service_secret_path_regex(
            common::USER_A_ID,
            project_id,
            &project_app.code,
            "S3_SECRET",
        )))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.vault_server)
        .await;

    reqwest::Client::new()
        .put(format!(
            "{}/project/{}/apps/{}/secrets/S3_SECRET",
            app.address, project_id, project_app.code
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "value": "supersecret" }))
        .send()
        .await
        .expect("service secret PUT failed");

    let response = reqwest::Client::new()
        .get(format!(
            "{}/project/{}/apps/{}/env",
            app.address, project_id, project_app.code
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("get env vars failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response
        .json()
        .await
        .expect("response body should be valid json");
    assert_eq!(body["item"]["variables"]["VISIBLE_KEY"], "plain-value");
    assert_eq!(body["item"]["variables"]["S3_SECRET"], "[REDACTED]");
    assert_eq!(
        body["item"]["runtime_env_contract"],
        serde_json::to_value(runtime_env_contract_response()).unwrap()
    );
}

#[tokio::test]
async fn test_get_app_redacts_plain_and_remote_secret_values() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let mut project_app = create_test_project_app(&app.db_pool, project_id, "web").await;
    project_app.environment = Some(json!({
        "VISIBLE_KEY": "plain-value",
        "LOCAL_PASSWORD": "db-secret"
    }));
    db::project_app::update(&app.db_pool, &project_app)
        .await
        .expect("project app update failed");

    let vault_path = format!(
        "agent/users/{}/projects/{}/apps/{}/secrets/S3_SECRET",
        common::USER_A_ID,
        project_id,
        project_app.code
    );
    db::remote_secret::upsert_service_secret(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        &project_app.code,
        "S3_SECRET",
        &vault_path,
        common::USER_A_ID,
        "synced",
    )
    .await
    .expect("service secret metadata insert failed");

    let response = reqwest::Client::new()
        .get(format!(
            "{}/project/{}/apps/{}",
            app.address, project_id, project_app.code
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("get app failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response
        .json()
        .await
        .expect("response body should be valid json");
    assert_eq!(body["item"]["environment"]["VISIBLE_KEY"], "plain-value");
    assert_eq!(body["item"]["environment"]["LOCAL_PASSWORD"], "[REDACTED]");
    assert_eq!(body["item"]["environment"]["S3_SECRET"], "[REDACTED]");
    assert_eq!(
        body["item"]["runtime_env_contract"],
        serde_json::to_value(runtime_env_contract_response()).unwrap()
    );
}

#[tokio::test]
async fn test_create_app_rejects_remote_secret_name_collision() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let vault_path = format!(
        "agent/users/{}/projects/{}/apps/{}/secrets/S3_SECRET",
        common::USER_A_ID,
        project_id,
        "web"
    );
    db::remote_secret::upsert_service_secret(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        "web",
        "S3_SECRET",
        &vault_path,
        common::USER_A_ID,
        "synced",
    )
    .await
    .expect("service secret metadata insert failed");

    let response = reqwest::Client::new()
        .post(format!("{}/project/{}/apps", app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({
            "code": "web",
            "image": "nginx:stable",
            "env": {
                "S3_SECRET": "plain-value",
                "VISIBLE_KEY": "plain-value"
            }
        }))
        .send()
        .await
        .expect("create app failed");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = response
        .text()
        .await
        .expect("response body should be readable");
    assert!(body.contains("managed as a remote service secret"));

    let created = db::project_app::fetch_by_project_and_code(&app.db_pool, project_id, "web")
        .await
        .expect("app fetch failed");
    assert!(created.is_none(), "conflicting app should not be created");
    assert_eq!(app.vault_server.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn test_update_env_vars_rejects_remote_secret_name_collision() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let project_app = create_test_project_app(&app.db_pool, project_id, "web").await;

    Mock::given(method("POST"))
        .and(path_regex(service_secret_path_regex(
            common::USER_A_ID,
            project_id,
            &project_app.code,
            "S3_SECRET",
        )))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.vault_server)
        .await;

    reqwest::Client::new()
        .put(format!(
            "{}/project/{}/apps/{}/secrets/S3_SECRET",
            app.address, project_id, project_app.code
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({ "value": "supersecret" }))
        .send()
        .await
        .expect("service secret PUT failed");

    let response = reqwest::Client::new()
        .put(format!(
            "{}/project/{}/apps/{}/env",
            app.address, project_id, project_app.code
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({
            "variables": {
                "S3_SECRET": "plain-value"
            }
        }))
        .send()
        .await
        .expect("update env vars failed");

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_mcp_get_app_env_vars_redacts_remote_service_secret_values() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let mut project_app = create_test_project_app(&app.db_pool, project_id, "web").await;
    project_app.environment = Some(json!({
        "VISIBLE_KEY": "plain-value",
        "LOCAL_PASSWORD": "db-secret"
    }));
    db::project_app::update(&app.db_pool, &project_app)
        .await
        .expect("project app update failed");

    let vault_path = format!(
        "agent/users/{}/projects/{}/apps/{}/secrets/S3_SECRET",
        common::USER_A_ID,
        project_id,
        project_app.code
    );
    db::remote_secret::upsert_service_secret(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        &project_app.code,
        "S3_SECRET",
        &vault_path,
        common::USER_A_ID,
        "synced",
    )
    .await
    .expect("service secret metadata insert failed");

    let context = tool_context(&app.db_pool);
    let result = GetAppEnvVarsTool
        .execute(
            json!({
                "project_id": project_id,
                "app_code": project_app.code
            }),
            &context,
        )
        .await
        .expect("mcp get_app_env_vars should succeed");

    let body = tool_text_json(result);
    assert_eq!(body["environment_variables"]["VISIBLE_KEY"], "plain-value");
    assert_eq!(
        body["environment_variables"]["LOCAL_PASSWORD"],
        "[REDACTED]"
    );
    assert_eq!(body["environment_variables"]["S3_SECRET"], "[REDACTED]");
    assert_eq!(body["count"], 3);
    assert_eq!(
        body["runtime_env_contract"],
        serde_json::to_value(runtime_env_contract_response()).unwrap()
    );
}

#[tokio::test]
async fn test_mcp_get_app_config_redacts_remote_service_secret_values() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let mut project_app = create_test_project_app(&app.db_pool, project_id, "web").await;
    project_app.environment = Some(json!({
        "VISIBLE_KEY": "plain-value",
        "LOCAL_PASSWORD": "db-secret"
    }));
    db::project_app::update(&app.db_pool, &project_app)
        .await
        .expect("project app update failed");

    let vault_path = format!(
        "agent/users/{}/projects/{}/apps/{}/secrets/S3_SECRET",
        common::USER_A_ID,
        project_id,
        project_app.code
    );
    db::remote_secret::upsert_service_secret(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        &project_app.code,
        "S3_SECRET",
        &vault_path,
        common::USER_A_ID,
        "synced",
    )
    .await
    .expect("service secret metadata insert failed");

    let context = tool_context(&app.db_pool);
    let result = GetAppConfigTool
        .execute(
            json!({
                "project_id": project_id,
                "app_code": project_app.code
            }),
            &context,
        )
        .await
        .expect("mcp get_app_config should succeed");

    let body = tool_text_json(result);
    assert_eq!(body["environment_variables"]["VISIBLE_KEY"], "plain-value");
    assert_eq!(
        body["environment_variables"]["LOCAL_PASSWORD"],
        "[REDACTED]"
    );
    assert_eq!(body["environment_variables"]["S3_SECRET"], "[REDACTED]");
    assert_eq!(
        body["runtime_env_contract"],
        serde_json::to_value(runtime_env_contract_response()).unwrap()
    );
}

#[tokio::test]
async fn test_mcp_set_app_env_var_rejects_remote_secret_name_collision() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let mut project_app = create_test_project_app(&app.db_pool, project_id, "web").await;
    project_app.environment = Some(json!({
        "VISIBLE_KEY": "plain-value"
    }));
    db::project_app::update(&app.db_pool, &project_app)
        .await
        .expect("project app update failed");

    let vault_path = format!(
        "agent/users/{}/projects/{}/apps/{}/secrets/S3_SECRET",
        common::USER_A_ID,
        project_id,
        project_app.code
    );
    db::remote_secret::upsert_service_secret(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        &project_app.code,
        "S3_SECRET",
        &vault_path,
        common::USER_A_ID,
        "synced",
    )
    .await
    .expect("service secret metadata insert failed");

    let context = tool_context(&app.db_pool);
    let error = SetAppEnvVarTool
        .execute(
            json!({
                "project_id": project_id,
                "app_code": project_app.code,
                "name": "S3_SECRET",
                "value": "plain-value"
            }),
            &context,
        )
        .await
        .expect_err("mcp set_app_env_var should reject remote secret collision");

    assert!(error.contains("managed as a remote service secret"));

    let updated = db::project_app::fetch_by_project_and_code(&app.db_pool, project_id, "web")
        .await
        .expect("app fetch failed")
        .expect("app should exist");
    let environment = updated.environment.expect("app environment should exist");
    assert_eq!(environment["VISIBLE_KEY"], "plain-value");
    assert!(environment.get("S3_SECRET").is_none());
}

#[tokio::test]
async fn test_mcp_get_remote_service_secret_marks_metadata_secure() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = common::create_test_project(&app.db_pool, common::USER_A_ID).await;
    let project_app = create_test_project_app(&app.db_pool, project_id, "web").await;
    let vault_path = format!(
        "agent/users/{}/projects/{}/apps/{}/secrets/S3_SECRET",
        common::USER_A_ID,
        project_id,
        project_app.code
    );
    db::remote_secret::upsert_service_secret(
        &app.db_pool,
        common::USER_A_ID,
        project_id,
        &project_app.code,
        "S3_SECRET",
        &vault_path,
        common::USER_A_ID,
        "synced",
    )
    .await
    .expect("service secret metadata insert failed");

    let context = tool_context(&app.db_pool);
    let result = GetRemoteServiceSecretTool
        .execute(
            json!({
                "project_id": project_id,
                "target_code": project_app.code,
                "name": "S3_SECRET"
            }),
            &context,
        )
        .await
        .expect("mcp get_remote_service_secret should succeed");

    let body = tool_text_json(result);
    assert_eq!(body["secret"]["name"], "S3_SECRET");
    assert_eq!(body["secret"]["source"], "vault");
    assert_eq!(body["secret"]["secure"], true);
}
