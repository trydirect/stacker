mod common;

use reqwest::StatusCode;
use serde_json::{json, Value};
use stacker::{db, models};

use tokio::sync::OnceCell;

static APP: OnceCell<common::TestApp> = OnceCell::const_new();

async fn app() -> &'static common::TestApp {
    common::get_or_init_app(&APP)
        .await
        .expect("Failed to start test app")
}

fn project_payload(stack_code: &str, service_codes: &[&str]) -> Value {
    let services = service_codes
        .iter()
        .enumerate()
        .map(|(idx, code)| {
            json!({
                "_id": format!("svc-{}", idx),
                "name": format!("{} service", code),
                "code": code,
                "type": "service",
                "custom": true,
                "dockerhub_image": format!("example/{code}:latest"),
                "domain": "",
                "restart": "unless-stopped",
                "network": ["default-network"],
                "environment": [{"key": "SERVICE_NAME", "value": code}],
                "shared_ports": [{"host_port": "", "container_port": "6379"}],
                "volumes": []
            })
        })
        .collect::<Vec<_>>();

    json!({
        "custom": {
            "custom_stack_code": stack_code,
            "project_name": format!("Project {stack_code}"),
            "networks": [{
                "id": "default-network",
                "name": "default_network"
            }],
            "web": [{
                "_id": "web-1",
                "name": "Website",
                "code": "website",
                "type": "web",
                "custom": true,
                "dockerhub_image": "nginx:1.27",
                "domain": "example.com",
                "restart": "always",
                "network": ["default-network"],
                "environment": [{"key": "PUBLIC_URL", "value": "https://example.com"}],
                "shared_ports": [{"host_port": "80", "container_port": "8080"}],
                "volumes": []
            }],
            "service": services,
            "feature": []
        }
    })
}

#[tokio::test]
async fn create_project_materializes_project_level_apps_from_form() {
    let Some(app) = common::spawn_app().await else {
        return;
    };

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/project", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&project_payload("sync-project-create", &["redis"]))
        .send()
        .await
        .expect("project create request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("response should be json");
    let project_id = body["item"]["id"]
        .as_i64()
        .expect("project id should be present") as i32;

    let apps = db::project_app::fetch_by_project(&app.db_pool, project_id)
        .await
        .expect("project apps should load");

    let codes = apps.iter().map(|app| app.code.as_str()).collect::<Vec<_>>();
    assert_eq!(codes, vec!["website", "redis"]);
    assert_eq!(apps[0].deployment_id, None);
    assert_eq!(
        apps[0].environment,
        Some(json!({"PUBLIC_URL": "https://example.com"}))
    );
    assert_eq!(apps[0].networks, Some(json!(["default_network"])));
    assert_eq!(apps[1].image, "example/redis:latest");
}

#[tokio::test]
async fn update_project_reconciles_project_level_apps_without_removing_deployment_rows() {
    let Some(app) = common::spawn_app().await else {
        return;
    };

    let client = reqwest::Client::new();
    let create_response = client
        .post(format!("{}/project", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&project_payload("sync-project-update", &["redis"]))
        .send()
        .await
        .expect("project create request should succeed");

    assert_eq!(create_response.status(), StatusCode::OK);
    let create_body: Value = create_response
        .json()
        .await
        .expect("create response should be json");
    let project_id = create_body["item"]["id"]
        .as_i64()
        .expect("project id should be present") as i32;

    let deployment_app = models::ProjectApp {
        deployment_id: Some(777),
        ..models::ProjectApp::new(
            project_id,
            "deployed-only".to_string(),
            "Deployment Only".to_string(),
            "example/deployed-only:latest".to_string(),
        )
    };
    db::project_app::insert(&app.db_pool, &deployment_app)
        .await
        .expect("deployment-scoped app should insert");

    let update_response = client
        .put(format!("{}/project/{}", app.address, project_id))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&project_payload("sync-project-update", &["upload"]))
        .send()
        .await
        .expect("project update request should succeed");

    assert_eq!(update_response.status(), StatusCode::OK);

    let apps = db::project_app::fetch_by_project(&app.db_pool, project_id)
        .await
        .expect("project apps should load");
    let project_level_codes = apps
        .iter()
        .filter(|app| app.deployment_id.is_none())
        .map(|app| app.code.as_str())
        .collect::<Vec<_>>();
    let deployment_codes = apps
        .iter()
        .filter(|app| app.deployment_id.is_some())
        .map(|app| app.code.as_str())
        .collect::<Vec<_>>();

    assert_eq!(project_level_codes, vec!["website", "upload"]);
    assert_eq!(deployment_codes, vec!["deployed-only"]);
}

#[tokio::test]
async fn sync_project_updates_apps_without_creating_a_deployment() {
    let Some(app) = common::spawn_app().await else {
        return;
    };

    let client = reqwest::Client::new();
    let create_response = client
        .post(format!("{}/project", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&project_payload("sync-project-endpoint", &["redis"]))
        .send()
        .await
        .expect("project create request should succeed");
    assert_eq!(create_response.status(), StatusCode::OK);

    let project_id = create_response
        .json::<Value>()
        .await
        .expect("create response should be json")["item"]["id"]
        .as_i64()
        .expect("project id should be present") as i32;

    let sync_response = client
        .put(format!(
            "{}/api/v1/project/{}/sync",
            app.address, project_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&project_payload("sync-project-endpoint", &["postgres"]))
        .send()
        .await
        .expect("project sync request should succeed");

    assert_eq!(sync_response.status(), StatusCode::OK);
    let body: Value = sync_response
        .json()
        .await
        .expect("sync response should be json");
    assert_eq!(body["item"]["deployment_created"], false);
    assert_eq!(body["item"]["server_contacted"], false);
    assert_eq!(body["item"]["containers_started"], false);

    let apps = db::project_app::fetch_by_project(&app.db_pool, project_id)
        .await
        .expect("project apps should load");
    let codes = apps.iter().map(|app| app.code.as_str()).collect::<Vec<_>>();
    assert_eq!(codes, vec!["website", "postgres"]);

    let deployment_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deployment WHERE project_id = $1 AND deleted = false",
    )
    .bind(project_id)
    .fetch_one(&app.db_pool)
    .await
    .expect("deployments should load");
    assert_eq!(deployment_count, 0, "sync must not create a deployment");
}

#[tokio::test]
async fn sync_project_persists_config_contract_on_apps() {
    let Some(app) = common::spawn_app().await else {
        return;
    };

    let client = reqwest::Client::new();
    let create_response = client
        .post(format!("{}/project", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({
            "custom": {
                "custom_stack_code": "sync-contract-project",
                "project_name": "Sync contract project",
                "networks": [{
                    "id": "default-network",
                    "name": "default_network"
                }],
                "web": [{
                    "_id": "web-1",
                    "name": "Website",
                    "code": "website",
                    "type": "web",
                    "custom": true,
                    "dockerhub_image": "nginx:1.27",
                    "domain": "example.com",
                    "restart": "always",
                    "network": ["default-network"],
                    "environment": [{"key": "JWT_SECRET", "value": "auto"}],
                    "shared_ports": [{"host_port": "80", "container_port": "8080"}],
                    "volumes": [],
                    "config_contract": {
                        "services": {
                            "web": {
                                "fields": {
                                    "JWT_SECRET": { "mutability": "generated" }
                                }
                            }
                        }
                    }
                }],
                "service": [],
                "feature": []
            }
        }))
        .send()
        .await
        .expect("project create request should succeed");
    assert_eq!(create_response.status(), StatusCode::OK);

    let project_id = create_response
        .json::<Value>()
        .await
        .expect("create response should be json")["item"]["id"]
        .as_i64()
        .expect("project id should be present") as i32;

    let apps = db::project_app::fetch_by_project(&app.db_pool, project_id)
        .await
        .expect("project apps should load");
    let website = apps
        .iter()
        .find(|app| app.code == "website")
        .expect("website app should exist");

    assert_eq!(
        website.config_contract,
        Some(json!({
            "services": {
                "web": {
                    "fields": {
                        "JWT_SECRET": { "mutability": "generated" }
                    }
                }
            }
        })),
        "config_contract should be persisted on the project app during sync"
    );
}
