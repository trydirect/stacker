mod common;

use reqwest::{Client, Response, StatusCode};
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn create_template(
    client: &Client,
    address: &str,
    token: &str,
    name: &str,
    slug: &str,
) -> Response {
    create_template_with_body(
        client,
        address,
        token,
        json!({
            "name": name,
            "slug": slug,
        }),
    )
    .await
}

async fn create_template_with_body(
    client: &Client,
    address: &str,
    token: &str,
    body: Value,
) -> Response {
    client
        .post(format!("{}/api/templates", address))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .expect("Failed to send template create request")
}

async fn list_my_templates(client: &Client, address: &str, token: &str) -> Value {
    client
        .get(format!("{}/api/templates/mine", address))
        .bearer_auth(token)
        .send()
        .await
        .expect("Failed to fetch my templates")
        .json()
        .await
        .expect("Templates response should be valid JSON")
}

struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            std::env::set_var(self.key, previous);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn marketplace_storage_test_configuration() -> stacker::configuration::Settings {
    let mut configuration =
        stacker::configuration::get_configuration().expect("Failed to get configuration");
    configuration.marketplace_assets.enabled = true;
    configuration.marketplace_assets.current_env = "test".to_string();
    configuration.marketplace_assets.endpoint_url = "https://objects.trydirect.test".to_string();
    configuration.marketplace_assets.region = "eu-central".to_string();
    configuration.marketplace_assets.access_key_id = "marketplace-test-access".to_string();
    configuration.marketplace_assets.secret_access_key = "marketplace-test-secret".to_string();
    configuration.marketplace_assets.bucket_test = "marketplace-assets-test".to_string();
    configuration.marketplace_assets.server_side_encryption = Some("AES256".to_string());
    configuration
}

#[tokio::test]
async fn create_template_with_duplicate_slug_by_different_user_returns_409() {
    let app = match common::spawn_app_two_users().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let first = create_template(
        &client,
        &app.address,
        common::USER_A_TOKEN,
        "User A Template",
        "duplicate-marketplace-slug",
    )
    .await;
    assert_eq!(StatusCode::CREATED, first.status());

    let second = create_template(
        &client,
        &app.address,
        common::USER_B_TOKEN,
        "User B Template",
        "duplicate-marketplace-slug",
    )
    .await;
    assert_eq!(StatusCode::CONFLICT, second.status());

    let body: Value = second
        .json()
        .await
        .expect("Conflict response should be valid JSON");
    let message = body["message"]
        .as_str()
        .expect("Conflict response should include a message");
    assert!(
        message.contains("already in use"),
        "Conflict message should explain the slug collision: {message}"
    );
    assert!(
        message.contains("duplicate-marketplace-slug"),
        "Conflict message should include the conflicting slug: {message}"
    );
}

#[tokio::test]
async fn create_template_same_slug_same_user_updates_existing_template() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let first = create_template(
        &client,
        &app.address,
        "test-bearer-token",
        "Original Template",
        "same-user-upsert-slug",
    )
    .await;
    assert_eq!(StatusCode::CREATED, first.status());
    let first_body: Value = first
        .json()
        .await
        .expect("Create response should be valid JSON");
    let first_id = first_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id")
        .to_string();

    let second = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Renamed Template",
            "slug": "same-user-upsert-slug",
            "version": "1.0.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.28" }
                }
            },
            "changelog": "Updated stack definition"
        }),
    )
    .await;
    assert_eq!(StatusCode::CREATED, second.status());
    let second_body: Value = second
        .json()
        .await
        .expect("Second create response should be valid JSON");
    let second_id = second_body["item"]["id"]
        .as_str()
        .expect("Updated template should include an id");
    assert_eq!(
        first_id, second_id,
        "Same-user duplicate slug should upsert"
    );
    assert_eq!(
        Some("Renamed Template"),
        second_body["item"]["name"].as_str(),
        "Second request should update template metadata"
    );
    let mine_response = client
        .get(format!("{}/api/templates/mine", app.address))
        .bearer_auth("test-bearer-token")
        .send()
        .await
        .expect("Failed to load templates after same-user upsert");
    assert_eq!(StatusCode::OK, mine_response.status());
    let mine_body: Value = mine_response
        .json()
        .await
        .expect("Mine response should be valid JSON");
    let updated_template = mine_body["list"]
        .as_array()
        .and_then(|templates| {
            templates
                .iter()
                .find(|template| template["slug"] == "same-user-upsert-slug")
        })
        .expect("Expected to find the upserted template");
    assert_eq!(Some("1.0.0"), updated_template["version"].as_str());
    assert_eq!(
        Some("Updated stack definition"),
        updated_template["changelog"].as_str()
    );
}

#[tokio::test]
async fn create_template_returns_infrastructure_requirements_when_provided() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Requirements Template",
            "slug": "requirements-template",
            "infrastructure_requirements": {
                "supported_clouds": ["hetzner", "aws"],
                "supported_os": ["ubuntu-22.04"],
                "min_ram_mb": 2048,
                "min_disk_gb": 20,
                "min_cpu_cores": 2
            }
        }),
    )
    .await;

    assert_eq!(StatusCode::CREATED, response.status());

    let body: Value = response
        .json()
        .await
        .expect("Create response should be valid JSON");

    assert_eq!(
        json!({
            "supported_clouds": ["hetzner", "aws"],
            "supported_os": ["ubuntu-22.04"],
            "min_ram_mb": 2048,
            "min_disk_gb": 20,
            "min_cpu_cores": 2
        }),
        body["item"]["infrastructure_requirements"]
    );
}

#[tokio::test]
async fn update_template_persists_infrastructure_requirements() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let create_response = create_template(
        &client,
        &app.address,
        "test-bearer-token",
        "Updatable Template",
        "updatable-template",
    )
    .await;
    assert_eq!(StatusCode::CREATED, create_response.status());
    let create_body: Value = create_response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = create_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id");

    let update_response = client
        .put(format!("{}/api/templates/{}", app.address, template_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "infrastructure_requirements": {
                "supported_clouds": ["digitalocean"],
                "min_ram_mb": 1024
            }
        }))
        .send()
        .await
        .expect("Failed to update template");
    assert_eq!(StatusCode::OK, update_response.status());

    let mine = list_my_templates(&client, &app.address, "test-bearer-token").await;
    let templates = mine["list"]
        .as_array()
        .expect("Mine response should contain a list");
    let template = templates
        .iter()
        .find(|template| template["slug"] == "updatable-template")
        .expect("Updated template should be present in my templates");

    assert_eq!(
        json!({
            "supported_clouds": ["digitalocean"],
            "min_ram_mb": 1024
        }),
        template["infrastructure_requirements"]
    );
}

#[tokio::test]
async fn create_template_persists_assets_from_initial_version_payload() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Asset Payload Template",
            "slug": "asset-payload-template",
            "version": "1.0.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.27" }
                }
            },
            "assets": [
                {
                    "storage_provider": "hetzner-object-storage",
                    "bucket": "marketplace-assets-test",
                    "key": "templates/template-1/versions/1.0.0/assets/abc12345/bundle.tgz",
                    "filename": "bundle.tgz",
                    "sha256": "abc12345",
                    "size": 2048,
                    "content_type": "application/gzip",
                    "decompress": true
                }
            ]
        }),
    )
    .await;

    assert_eq!(StatusCode::CREATED, response.status());

    let body: Value = response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = body["item"]["id"]
        .as_str()
        .expect("Created template should include an id");

    let persisted_assets: Value = sqlx::query_scalar(
        r#"SELECT assets FROM stack_template_version WHERE template_id = $1::uuid AND is_latest = true"#,
    )
    .bind(Uuid::parse_str(template_id).expect("Template id should be a UUID"))
    .fetch_one(&app.db_pool)
    .await
    .expect("Expected latest version assets to persist");

    assert_eq!(
        json!([
            {
                "storage_provider": "hetzner-object-storage",
                "bucket": "marketplace-assets-test",
                "key": "templates/template-1/versions/1.0.0/assets/abc12345/bundle.tgz",
                "filename": "bundle.tgz",
                "sha256": "abc12345",
                "size": 2048,
                "content_type": "application/gzip",
                "decompress": true
            }
        ]),
        persisted_assets
    );
}

#[tokio::test]
async fn list_my_templates_includes_latest_version_contract_fields() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Versioned Template",
            "slug": "versioned-template",
            "version": "1.3.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.27" }
                }
            },
            "changelog": "Adds runtime contract fields",
            "config_files": [
                {"path": "/etc/app/config.yaml", "content": "runtimeBundle: enabled"}
            ],
            "assets": [
                {
                    "storage_provider": "hetzner-object-storage",
                    "bucket": "marketplace-assets-test",
                    "key": "templates/versioned-template/versions/1.3.0/assets/abc12345/bundle.tgz",
                    "filename": "bundle.tgz",
                    "sha256": "abc12345",
                    "size": 1024,
                    "content_type": "application/gzip",
                    "decompress": true
                }
            ],
            "seed_jobs": [
                {"name": "seed-db", "command": "bin/seed-db.sh"}
            ],
            "post_deploy_hooks": [
                {"name": "warm-cache", "command": "bin/warm-cache.sh"}
            ],
            "update_mode_capabilities": {
                "mode_self_managed": true,
                "mode_managed_status_panel": true
            }
        }),
    )
    .await;
    assert_eq!(StatusCode::CREATED, response.status());

    let mine = list_my_templates(&client, &app.address, "test-bearer-token").await;
    let template = mine["list"]
        .as_array()
        .and_then(|templates| templates.iter().find(|template| template["slug"] == "versioned-template"))
        .expect("Versioned template should be listed");

    assert_eq!(json!("1.3.0"), template["version"]);
    assert_eq!(json!("Adds runtime contract fields"), template["changelog"]);
    assert_eq!(
        json!([{ "path": "/etc/app/config.yaml", "content": "runtimeBundle: enabled" }]),
        template["config_files"]
    );
    assert_eq!(json!(1), template["assets"].as_array().map(|items| items.len()).unwrap_or(0));
    assert_eq!(json!(1), template["seed_jobs"].as_array().map(|items| items.len()).unwrap_or(0));
    assert_eq!(json!(1), template["post_deploy_hooks"].as_array().map(|items| items.len()).unwrap_or(0));
    assert_eq!(
        json!({
            "mode_self_managed": true,
            "mode_managed_status_panel": true
        }),
        template["update_mode_capabilities"]
    );
}

#[tokio::test]
async fn update_template_persists_latest_version_contract_fields_for_drafts() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let create_response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Draft Contract Update Template",
            "slug": "draft-contract-update-template",
            "version": "1.0.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.27" }
                }
            }
        }),
    )
    .await;
    assert_eq!(StatusCode::CREATED, create_response.status());

    let create_body: Value = create_response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = create_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id");

    let update_response = client
        .put(format!("{}/api/templates/{}", app.address, template_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "version": "1.2.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.28" }
                }
            },
            "category_code": "developer-tools",
            "definition_format": "json",
            "changelog": "Adds draft update persistence",
            "required_plan_name": "professional",
            "config_files": [
                {
                    "path": "/etc/app/config.yaml",
                    "content": "runtimeBundle: enabled"
                }
            ],
            "assets": [
                {
                    "storage_provider": "hetzner-object-storage",
                    "bucket": "marketplace-assets-test",
                    "key": "templates/template-1/versions/1.2.0/assets/def67890/bundle.tgz",
                    "filename": "bundle.tgz",
                    "sha256": "def67890",
                    "size": 4096,
                    "content_type": "application/gzip",
                    "decompress": true
                }
            ],
            "seed_jobs": [
                {
                    "name": "seed-db",
                    "command": "bin/seed-db.sh"
                }
            ],
            "post_deploy_hooks": [
                {
                    "name": "warm-cache",
                    "command": "bin/warm-cache.sh"
                }
            ],
            "update_mode_capabilities": {
                "mode_self_managed": true,
                "mode_managed_status_panel": true
            }
        }))
        .send()
        .await
        .expect("Failed to update template");
    assert_eq!(StatusCode::OK, update_response.status());

    let latest_version: Value = sqlx::query_scalar(
        r#"SELECT jsonb_build_object(
                'version', version,
                'stack_definition', stack_definition,
                'definition_format', definition_format,
                'changelog', changelog,
                'config_files', config_files,
                'assets', assets,
                'seed_jobs', seed_jobs,
                'post_deploy_hooks', post_deploy_hooks,
                'update_mode_capabilities', update_mode_capabilities
            )
            FROM stack_template_version
            WHERE template_id = $1::uuid AND is_latest = true"#,
    )
    .bind(Uuid::parse_str(template_id).expect("Template id should be a UUID"))
    .fetch_one(&app.db_pool)
    .await
    .expect("Expected latest version contract to persist");

    assert_eq!(json!("1.2.0"), latest_version["version"]);
    assert_eq!(json!("json"), latest_version["definition_format"]);
    assert_eq!(json!("Adds draft update persistence"), latest_version["changelog"]);
    assert_eq!(
        json!([{ "path": "/etc/app/config.yaml", "content": "runtimeBundle: enabled" }]),
        latest_version["config_files"]
    );
    assert_eq!(
        json!([
            {
                "storage_provider": "hetzner-object-storage",
                "bucket": "marketplace-assets-test",
                "key": "templates/template-1/versions/1.2.0/assets/def67890/bundle.tgz",
                "filename": "bundle.tgz",
                "sha256": "def67890",
                "size": 4096,
                "content_type": "application/gzip",
                "decompress": true
            }
        ]),
        latest_version["assets"]
    );
    assert_eq!(
        json!([{ "name": "seed-db", "command": "bin/seed-db.sh" }]),
        latest_version["seed_jobs"]
    );
    assert_eq!(
        json!([{ "name": "warm-cache", "command": "bin/warm-cache.sh" }]),
        latest_version["post_deploy_hooks"]
    );
    assert_eq!(
        json!({
            "mode_self_managed": true,
            "mode_managed_status_panel": true
        }),
        latest_version["update_mode_capabilities"]
    );

    let template_fields: Value = sqlx::query_scalar(
        r#"SELECT jsonb_build_object(
                'category_code', c.name,
                'required_plan_name', t.required_plan_name
            )
            FROM stack_template t
            LEFT JOIN stack_category c ON c.id = t.category_id
            WHERE t.id = $1::uuid"#,
    )
    .bind(Uuid::parse_str(template_id).expect("Template id should be a UUID"))
    .fetch_one(&app.db_pool)
    .await
    .expect("Expected template metadata to persist");

    assert_eq!(json!("developer-tools"), template_fields["category_code"]);
    assert_eq!(json!("professional"), template_fields["required_plan_name"]);
}

#[tokio::test]
async fn create_template_returns_public_ports_when_provided() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Ports Template",
            "slug": "ports-template",
            "public_ports": [
                {"name": "web", "port": 8080},
                {"name": "https", "port": 443}
            ]
        }),
    )
    .await;

    assert_eq!(StatusCode::CREATED, response.status());

    let body: Value = response
        .json()
        .await
        .expect("Create response should be valid JSON");

    assert_eq!(
        json!([
            {"name": "web", "port": 8080},
            {"name": "https", "port": 443}
        ]),
        body["item"]["public_ports"]
    );
}

#[tokio::test]
async fn create_template_returns_vendor_url_when_provided() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Vendor Template",
            "slug": "vendor-template",
            "vendor_url": "https://example.com"
        }),
    )
    .await;

    assert_eq!(StatusCode::CREATED, response.status());

    let body: Value = response
        .json()
        .await
        .expect("Create response should be valid JSON");

    assert_eq!(
        "https://example.com",
        body["item"]["vendor_url"]
            .as_str()
            .expect("vendor_url should be a string")
    );
}

#[tokio::test]
async fn update_template_persists_public_ports_and_vendor_url() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let create_response = create_template(
        &client,
        &app.address,
        "test-bearer-token",
        "Updatable Template",
        "updatable-ports-template",
    )
    .await;
    assert_eq!(StatusCode::CREATED, create_response.status());

    let template_id = create_response
        .json::<Value>()
        .await
        .expect("Create response should be valid JSON")["item"]["id"]
        .as_str()
        .expect("id should be a string")
        .to_string();

    let update_response = client
        .put(format!("{}/api/templates/{}", app.address, template_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "public_ports": [{"name": "app", "port": 3000}],
            "vendor_url": "https://app.example.com/docs"
        }))
        .send()
        .await
        .expect("Failed to send template update request");

    assert_eq!(StatusCode::OK, update_response.status());

    update_response
        .json::<Value>()
        .await
        .expect("Update response should be valid JSON");

    let response = list_my_templates(&client, &app.address, "test-bearer-token").await;
    let template = response
        .get("list")
        .and_then(|list| list.get(0))
        .expect("Should return updated template");

    assert_eq!(
        json!([{"name": "app", "port": 3000}]),
        template["public_ports"]
    );
    assert_eq!(
        "https://app.example.com/docs",
        template["vendor_url"]
            .as_str()
            .expect("vendor_url should be a string")
    );
}

#[tokio::test]
async fn submit_template_sends_template_submitted_webhook() {
    let _env_lock = env_lock().lock().expect("env lock should be available");
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();
    let mock_user_service = MockServer::start().await;
    let _url_server_user = EnvGuard::set("URL_SERVER_USER", &mock_user_service.uri());
    let _user_service_url = EnvGuard::set("USER_SERVICE_URL", &mock_user_service.uri());
    let _user_service_base_url = EnvGuard::set("USER_SERVICE_BASE_URL", &mock_user_service.uri());
    let _stacker_service_token = EnvGuard::set("STACKER_SERVICE_TOKEN", "stacker-test-token");

    Mock::given(method("POST"))
        .and(path("/marketplace/sync"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "message": "ok",
            "product_id": null
        })))
        .mount(&mock_user_service)
        .await;

    let create_response = create_template(
        &client,
        &app.address,
        "test-bearer-token",
        "Submit Notification Template",
        "submit-notification-template",
    )
    .await;
    assert_eq!(StatusCode::CREATED, create_response.status());
    let create_body: Value = create_response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = create_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id");

    let submit_response = client
        .post(format!(
            "{}/api/templates/{}/submit",
            app.address, template_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&serde_json::json!({
            "confirm_no_secrets": true
        }))
        .send()
        .await
        .expect("Failed to submit template for review");
    assert_eq!(StatusCode::OK, submit_response.status());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let requests = mock_user_service
        .received_requests()
        .await
        .expect("Should capture webhook request");
    let webhook_request = requests
        .iter()
        .find(|request| request.url.path() == "/marketplace/sync")
        .expect("Submit should send marketplace webhook");
    let payload: Value =
        serde_json::from_slice(&webhook_request.body).expect("Webhook body should be valid JSON");

    assert_eq!("template_submitted", payload["action"]);
    assert_eq!(template_id, payload["stack_template_id"]);
    assert_eq!("submit-notification-template", payload["code"]);
}

#[tokio::test]
async fn asset_presign_and_finalize_persist_marketplace_asset_metadata() {
    let app = match common::spawn_app_with_test_auth_configuration(
        marketplace_storage_test_configuration(),
    )
    .await
    {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let create_response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Asset Template",
            "slug": "asset-template",
            "version": "1.0.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.27" }
                }
            }
        }),
    )
    .await;
    assert_eq!(StatusCode::CREATED, create_response.status());

    let create_body: Value = create_response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = create_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id")
        .to_string();

    let presign_response = client
        .post(format!(
            "{}/api/v1/templates/{}/assets/presign",
            app.address, template_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "filename": "bundle.tgz",
            "sha256": "abc12345",
            "size": 2048,
            "content_type": "application/gzip",
            "fetch_target": "/bootstrap/bundle.tgz",
            "immutable": true
        }))
        .send()
        .await
        .expect("Failed to presign asset upload");
    assert_eq!(StatusCode::OK, presign_response.status());

    let presign_body: Value = presign_response
        .json()
        .await
        .expect("Presign response should be valid JSON");
    let asset = presign_body["item"]["asset"].clone();
    let asset_key = asset["key"]
        .as_str()
        .expect("Asset key should be returned")
        .to_string();

    assert_eq!("PUT", presign_body["item"]["method"]);
    assert_eq!(
        Some("AES256"),
        presign_body["item"]["headers"]["x-amz-server-side-encryption"].as_str()
    );
    assert!(presign_body["item"]["url"]
        .as_str()
        .expect("Presigned upload URL should be a string")
        .contains("X-Amz-Signature="));
    assert!(
        asset_key.contains("/versions/1.0.0/assets/abc12345/bundle.tgz"),
        "Asset key should use the immutable versioned layout"
    );

    let finalize_response = client
        .post(format!(
            "{}/api/v1/templates/{}/assets/finalize",
            app.address, template_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "bucket": asset["bucket"],
            "key": asset["key"],
            "filename": asset["filename"],
            "sha256": asset["sha256"],
            "size": asset["size"],
            "content_type": asset["content_type"],
            "fetch_target": asset["fetch_target"],
            "immutable": true
        }))
        .send()
        .await
        .expect("Failed to finalize asset upload");
    assert_eq!(StatusCode::OK, finalize_response.status());

    let finalize_body: Value = finalize_response
        .json()
        .await
        .expect("Finalize response should be valid JSON");
    let persisted_assets = finalize_body["item"]
        .as_array()
        .expect("Finalize should return the latest version asset list");
    assert_eq!(1, persisted_assets.len());
    assert_eq!(asset_key, persisted_assets[0]["key"]);

    sqlx::query(
        r#"UPDATE stack_template SET status = 'approved', approved_at = NOW() WHERE id = $1"#,
    )
    .bind(Uuid::parse_str(&template_id).expect("Template id should be a UUID"))
    .execute(&app.db_pool)
    .await
    .expect("Failed to mark template approved");

    let detail_response = client
        .get(format!("{}/api/v1/templates/asset-template", app.address))
        .send()
        .await
        .expect("Failed to fetch template detail");
    assert_eq!(StatusCode::OK, detail_response.status());

    let detail_body: Value = detail_response
        .json()
        .await
        .expect("Detail response should be valid JSON");
    let detail_assets = detail_body["item"]["latest_version"]["assets"]
        .as_array()
        .expect("Approved template detail should expose latest version assets");
    assert_eq!(1, detail_assets.len());
    assert_eq!(asset_key, detail_assets[0]["key"]);

    let download_response = client
        .post(format!(
            "{}/api/v1/templates/{}/assets/presign-download",
            app.address, template_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({ "key": asset_key }))
        .send()
        .await
        .expect("Failed to presign asset download");
    assert_eq!(StatusCode::OK, download_response.status());

    let download_body: Value = download_response
        .json()
        .await
        .expect("Download presign response should be valid JSON");
    assert_eq!("GET", download_body["item"]["method"]);
    assert!(download_body["item"]["url"]
        .as_str()
        .expect("Presigned download URL should be a string")
        .contains("X-Amz-Signature="));
}

#[tokio::test]
async fn approved_template_assets_are_read_only() {
    let app = match common::spawn_app_with_test_auth_configuration(
        marketplace_storage_test_configuration(),
    )
    .await
    {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let create_response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Approved Asset Template",
            "slug": "approved-asset-template",
            "version": "1.0.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.27" }
                }
            }
        }),
    )
    .await;
    assert_eq!(StatusCode::CREATED, create_response.status());

    let create_body: Value = create_response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = create_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id")
        .to_string();

    sqlx::query(
        r#"UPDATE stack_template SET status = 'approved', approved_at = NOW() WHERE id = $1"#,
    )
    .bind(Uuid::parse_str(&template_id).expect("Template id should be a UUID"))
    .execute(&app.db_pool)
    .await
    .expect("Failed to mark template approved");

    let presign_response = client
        .post(format!(
            "{}/api/v1/templates/{}/assets/presign",
            app.address, template_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "filename": "bundle.tgz",
            "sha256": "abc12345",
            "size": 2048
        }))
        .send()
        .await
        .expect("Failed to presign asset upload");
    assert_eq!(StatusCode::BAD_REQUEST, presign_response.status());
    let presign_body: Value = presign_response
        .json()
        .await
        .expect("Presign error should be valid JSON");
    assert!(presign_body["message"]
        .as_str()
        .expect("Error response should include a message")
        .contains("read-only"));

    let finalize_response = client
        .post(format!(
            "{}/api/v1/templates/{}/assets/finalize",
            app.address, template_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "bucket": "marketplace-assets-test",
            "key": "templates/fake/versions/1.0.0/assets/abc12345/bundle.tgz",
            "filename": "bundle.tgz",
            "sha256": "abc12345",
            "size": 2048
        }))
        .send()
        .await
        .expect("Failed to finalize asset upload");
    assert_eq!(StatusCode::BAD_REQUEST, finalize_response.status());
    let finalize_body: Value = finalize_response
        .json()
        .await
        .expect("Finalize error should be valid JSON");
    assert!(finalize_body["message"]
        .as_str()
        .expect("Error response should include a message")
        .contains("read-only"));
}

#[tokio::test]
async fn finalize_asset_rejects_tampered_bucket_or_key() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let create_response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Tamper Guard Template",
            "slug": "tamper-guard-template",
            "version": "1.0.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:latest" }
                }
            }
        }),
    )
    .await;
    assert_eq!(StatusCode::CREATED, create_response.status());

    let create_body: Value = create_response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = create_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id")
        .to_string();

    let presign_response = client
        .post(format!(
            "{}/api/v1/templates/{}/assets/presign",
            app.address, template_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "filename": "bundle.tgz",
            "sha256": "abc12345",
            "size": 2048,
            "content_type": "application/gzip"
        }))
        .send()
        .await
        .expect("Failed to presign asset upload");
    assert_eq!(StatusCode::OK, presign_response.status());

    let presign_body: Value = presign_response
        .json()
        .await
        .expect("Presign response should be valid JSON");
    let asset = presign_body["item"]["asset"].clone();

    let finalize_response = client
        .post(format!(
            "{}/api/v1/templates/{}/assets/finalize",
            app.address, template_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "bucket": "marketplace-assets-prod",
            "key": "templates/other-template/versions/1.0.0/assets/abc12345/bundle.tgz",
            "filename": asset["filename"],
            "sha256": asset["sha256"],
            "size": asset["size"],
            "content_type": asset["content_type"]
        }))
        .send()
        .await
        .expect("Failed to finalize tampered asset upload");
    assert_eq!(StatusCode::BAD_REQUEST, finalize_response.status());
}

#[tokio::test]
async fn create_template_persists_extended_version_contract_in_public_detail() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = Client::new();

    let response = create_template_with_body(
        &client,
        &app.address,
        "test-bearer-token",
        json!({
            "name": "Contract Mirror Template",
            "slug": "contract-mirror-template",
            "version": "1.0.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.27" }
                }
            },
            "definition_format": "json",
            "changelog": "Initial marketplace release with config files and lifecycle hooks.",
            "config_files": [
                {
                    "path": "/etc/nginx/conf.d/default.conf",
                    "content": "server { listen 80; server_name _; }",
                    "encoding": "utf-8",
                    "mode": "0644",
                    "target_service": "nginx",
                    "description": "Base nginx vhost config"
                }
            ],
            "seed_jobs": [
                {
                    "name": "db-seed",
                    "image": "postgres:16",
                    "command": ["psql", "-f", "/bootstrap/init.sql"],
                    "execution_scope": "container",
                    "timeout_seconds": 900,
                    "retry_limit": 1,
                    "env": {
                        "PGDATABASE": "app"
                    },
                    "idempotency_key": "seed-db-v1"
                }
            ],
            "post_deploy_hooks": [
                {
                    "name": "warm-cache",
                    "command": "node scripts/warm-cache.js",
                    "execution_scope": "container",
                    "timeout_seconds": 300,
                    "retry_limit": 0,
                    "env": {
                        "NODE_ENV": "production"
                    },
                    "idempotency_key": "warm-cache-v1"
                }
            ],
            "update_mode_capabilities": {
                "mode_self_managed": true,
                "mode_managed_status_panel": true,
                "supports_rollback": true,
                "requires_backup": true,
                "backup_providers": ["syncopia", "snapshot"]
            }
        }),
    )
    .await;

    assert_eq!(StatusCode::CREATED, response.status());

    let body: Value = response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = body["item"]["id"]
        .as_str()
        .expect("Created template should include an id");

    sqlx::query(
        r#"UPDATE stack_template SET status = 'approved', approved_at = NOW() WHERE id = $1"#,
    )
    .bind(Uuid::parse_str(template_id).expect("Template id should be a UUID"))
    .execute(&app.db_pool)
    .await
    .expect("Failed to mark template approved");

    let detail_response = client
        .get(format!(
            "{}/api/v1/templates/contract-mirror-template",
            app.address
        ))
        .send()
        .await
        .expect("Failed to fetch template detail");
    assert_eq!(StatusCode::OK, detail_response.status());

    let detail_body: Value = detail_response
        .json()
        .await
        .expect("Detail response should be valid JSON");
    let latest_version = &detail_body["item"]["latest_version"];

    assert_eq!(
        "Initial marketplace release with config files and lifecycle hooks.",
        latest_version["changelog"]
    );
    assert_eq!(
        json!([
            {
                "path": "/etc/nginx/conf.d/default.conf",
                "content": "server { listen 80; server_name _; }",
                "encoding": "utf-8",
                "mode": "0644",
                "target_service": "nginx",
                "description": "Base nginx vhost config"
            }
        ]),
        latest_version["config_files"]
    );
    assert_eq!(
        json!([
            {
                "name": "db-seed",
                "image": "postgres:16",
                "command": ["psql", "-f", "/bootstrap/init.sql"],
                "execution_scope": "container",
                "timeout_seconds": 900,
                "retry_limit": 1,
                "env": {
                    "PGDATABASE": "app"
                },
                "idempotency_key": "seed-db-v1"
            }
        ]),
        latest_version["seed_jobs"]
    );
    assert_eq!(
        json!([
            {
                "name": "warm-cache",
                "command": "node scripts/warm-cache.js",
                "execution_scope": "container",
                "timeout_seconds": 300,
                "retry_limit": 0,
                "env": {
                    "NODE_ENV": "production"
                },
                "idempotency_key": "warm-cache-v1"
            }
        ]),
        latest_version["post_deploy_hooks"]
    );
    assert_eq!(
        json!({
            "mode_self_managed": true,
            "mode_managed_status_panel": true,
            "supports_rollback": true,
            "requires_backup": true,
            "backup_providers": ["syncopia", "snapshot"]
        }),
        latest_version["update_mode_capabilities"]
    );
}
