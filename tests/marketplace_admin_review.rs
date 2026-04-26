mod common;

use chrono::{Duration, Utc};
use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::{Mutex, OnceLock};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn create_admin_jwt() -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let header = json!({"alg": "HS256", "typ": "JWT"});
    let payload = json!({
        "role": "admin_service",
        "email": "ops@test.com",
        "exp": (Utc::now() + Duration::minutes(30)).timestamp(),
    });

    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string());

    format!("{}.{}.{}", header_b64, payload_b64, "test_signature")
}

async fn insert_template(
    pool: &sqlx::PgPool,
    creator_user_id: &str,
    slug: &str,
    status: &str,
) -> String {
    sqlx::query(
        r#"INSERT INTO stack_template (
            creator_user_id,
            creator_name,
            name,
            slug,
            status,
            tags,
            tech_stack
        )
        VALUES ($1, 'Creator Example', 'Review Template', $2, $3, '[]'::jsonb, '{}'::jsonb)
        RETURNING id"#,
    )
    .bind(creator_user_id)
    .bind(slug)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("Failed to insert template")
    .get::<uuid::Uuid, _>("id")
    .to_string()
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

#[tokio::test]
async fn admin_can_mark_template_needs_changes_and_creator_can_see_reason() {
    // Hold env_lock to prevent concurrent webhook-triggering tests from cross-contaminating
    // each other's mock servers via shared env vars (URL_SERVER_USER etc.)
    let _env_lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "needs-changes-review-template",
        "submitted",
    )
    .await;

    let admin_response = reqwest::Client::new()
        .post(format!(
            "{}/api/admin/templates/{}/needs-changes",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "reason": "Please document the required Hetzner bare metal prerequisites."
        }))
        .send()
        .await
        .expect("Failed to send admin needs-changes request");

    assert_eq!(StatusCode::OK, admin_response.status());

    let template_status =
        sqlx::query_scalar::<_, String>(r#"SELECT status FROM stack_template WHERE id = $1::uuid"#)
            .bind(uuid::Uuid::parse_str(&template_id).expect("template id should be a uuid"))
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to fetch updated template status");

    assert_eq!("needs_changes", template_status);

    let reviews_response = reqwest::Client::new()
        .get(format!(
            "{}/api/templates/{}/reviews",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to fetch creator reviews");

    assert_eq!(StatusCode::OK, reviews_response.status());

    let body: Value = reviews_response
        .json()
        .await
        .expect("reviews response should be valid JSON");
    let latest_review = &body["list"][0];

    assert_eq!("needs_changes", latest_review["decision"]);
    assert_eq!(
        "Please document the required Hetzner bare metal prerequisites.",
        latest_review["review_reason"]
    );
}

#[tokio::test]
async fn admin_cannot_mark_approved_template_as_needs_changes() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "approved-template-needs-changes-blocked",
        "approved",
    )
    .await;

    let admin_response = reqwest::Client::new()
        .post(format!(
            "{}/api/admin/templates/{}/needs-changes",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "reason": "Please update the deployment guide."
        }))
        .send()
        .await
        .expect("Failed to send admin needs-changes request");

    assert_eq!(StatusCode::BAD_REQUEST, admin_response.status());

    let template_status =
        sqlx::query_scalar::<_, String>(r#"SELECT status FROM stack_template WHERE id = $1::uuid"#)
            .bind(uuid::Uuid::parse_str(&template_id).expect("template id should be a uuid"))
            .fetch_one(&app.db_pool)
            .await
            .expect("Failed to fetch template status");

    assert_eq!("approved", template_status);
}

#[tokio::test]
async fn admin_approval_sends_template_published_webhook() {
    let _env_lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
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

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "published-webhook-template",
        "submitted",
    )
    .await;

    let admin_response = reqwest::Client::new()
        .post(format!(
            "{}/api/admin/templates/{}/approve",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "decision": "approved",
            "reason": "Looks good."
        }))
        .send()
        .await
        .expect("Failed to send admin approval request");

    assert_eq!(StatusCode::OK, admin_response.status());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let requests = mock_user_service
        .received_requests()
        .await
        .expect("Should capture webhook request");
    let webhook_request = requests
        .iter()
        .find(|request| request.url.path() == "/marketplace/sync")
        .expect("Approval should send marketplace webhook");
    let payload: Value =
        serde_json::from_slice(&webhook_request.body).expect("Webhook body should be valid JSON");

    assert_eq!("template_published", payload["action"]);
    assert_eq!(template_id, payload["stack_template_id"]);
    assert_eq!("published-webhook-template", payload["code"]);
}

#[tokio::test]
async fn admin_rejection_sends_template_review_rejected_webhook() {
    let _env_lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
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

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "review-rejected-template",
        "submitted",
    )
    .await;

    let admin_response = reqwest::Client::new()
        .post(format!(
            "{}/api/admin/templates/{}/reject",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "decision": "rejected",
            "reason": "The submission does not meet marketplace quality standards yet."
        }))
        .send()
        .await
        .expect("Failed to send admin rejection request");

    assert_eq!(StatusCode::OK, admin_response.status());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let requests = mock_user_service
        .received_requests()
        .await
        .expect("Should capture webhook request");
    let webhook_request = requests
        .iter()
        .find(|request| request.url.path() == "/marketplace/sync")
        .expect("Rejection should send marketplace webhook");
    let payload: Value =
        serde_json::from_slice(&webhook_request.body).expect("Webhook body should be valid JSON");

    assert_eq!("template_review_rejected", payload["action"]);
    assert_eq!(template_id, payload["stack_template_id"]);
    assert_eq!(
        "The submission does not meet marketplace quality standards yet.",
        payload["review_reason"]
    );
}

#[tokio::test]
async fn admin_unapprove_sends_template_unpublished_webhook() {
    let _env_lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
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

    let template_id = insert_template(
        &app.db_pool,
        common::USER_A_ID,
        "unpublished-template",
        "approved",
    )
    .await;

    let admin_response = reqwest::Client::new()
        .post(format!(
            "{}/api/admin/templates/{}/unapprove",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .json(&json!({
            "reason": "Temporarily hidden from the marketplace."
        }))
        .send()
        .await
        .expect("Failed to send admin unapprove request");

    assert_eq!(StatusCode::OK, admin_response.status());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let requests = mock_user_service
        .received_requests()
        .await
        .expect("Should capture webhook request");
    let webhook_request = requests
        .iter()
        .find(|request| request.url.path() == "/marketplace/sync")
        .expect("Unapprove should send marketplace webhook");
    let payload: Value =
        serde_json::from_slice(&webhook_request.body).expect("Webhook body should be valid JSON");

    assert_eq!("template_unpublished", payload["action"]);
    assert_eq!(template_id, payload["stack_template_id"]);
}

#[tokio::test]
async fn admin_detail_lists_extended_version_contract_for_resubmitted_templates() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let create_response = client
        .post(format!("{}/api/templates", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({
            "name": "Admin Contract Mirror Template",
            "slug": "admin-contract-mirror-template",
            "version": "1.0.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.27" }
                }
            },
            "definition_format": "json",
            "changelog": "Initial version",
            "config_files": [
                {
                    "path": "/etc/app/config.yaml",
                    "content": "managedUpdates: true",
                    "encoding": "utf-8",
                    "mode": "0644"
                }
            ],
            "seed_jobs": [
                {
                    "name": "seed-v1",
                    "command": "bin/seed-v1.sh",
                    "timeout_seconds": 120
                }
            ],
            "post_deploy_hooks": [
                {
                    "name": "warm-cache-v1",
                    "command": "bin/warm-cache-v1.sh",
                    "execution_scope": "container"
                }
            ],
            "assets": [
                {
                    "storage_provider": "hetzner-object-storage",
                    "bucket": "marketplace-assets-test",
                    "key": "templates/admin-contract-mirror-template/versions/1.0.0/assets/abc12345/bundle-v1.tgz",
                    "filename": "bundle-v1.tgz",
                    "sha256": "abc12345",
                    "size": 512,
                    "content_type": "application/gzip",
                    "decompress": true
                }
            ],
            "update_mode_capabilities": {
                "mode_self_managed": true,
                "mode_managed_status_panel": false,
                "supports_rollback": true,
                "requires_backup": false,
                "backup_providers": []
            }
        }))
        .send()
        .await
        .expect("Failed to create marketplace template");
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
    .bind(uuid::Uuid::parse_str(&template_id).expect("Template id should be a UUID"))
    .execute(&app.db_pool)
    .await
    .expect("Failed to approve template for resubmission");

    let resubmit_response = client
        .post(format!(
            "{}/api/templates/{}/resubmit",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({
            "name": "Admin Contract Mirror Template v2",
            "short_description": "Updated short description",
            "long_description": "Updated long description",
            "category_code": "developer-tools",
            "version": "1.1.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.28" },
                    "worker": { "image": "alpine:3.20" }
                }
            },
            "definition_format": "json",
            "changelog": "Adds worker seed and managed status panel updates.",
            "plan_type": "subscription",
            "required_plan_name": "professional",
            "price": 29.0,
            "currency": "USD",
            "config_files": [
                {
                    "path": "/etc/app/config.yaml",
                    "content": "managedUpdates: true\nworker: enabled",
                    "encoding": "utf-8",
                    "mode": "0644"
                }
            ],
            "seed_jobs": [
                {
                    "name": "seed-v2",
                    "command": ["bin/seed-v2.sh", "--force"],
                    "timeout_seconds": 240,
                    "retry_limit": 1
                }
            ],
            "post_deploy_hooks": [
                {
                    "name": "warm-cache-v2",
                    "command": "bin/warm-cache-v2.sh",
                    "execution_scope": "container",
                    "timeout_seconds": 90
                }
            ],
            "assets": [
                {
                    "storage_provider": "hetzner-object-storage",
                    "bucket": "marketplace-assets-test",
                    "key": "templates/admin-contract-mirror-template/versions/1.1.0/assets/def67890/bundle-v2.tgz",
                    "filename": "bundle-v2.tgz",
                    "sha256": "def67890",
                    "size": 1024,
                    "content_type": "application/gzip",
                    "decompress": true
                }
            ],
            "update_mode_capabilities": {
                "mode_self_managed": true,
                "mode_managed_status_panel": true,
                "supports_rollback": true,
                "requires_backup": true,
                "backup_providers": ["snapshot"]
            },
            "confirm_no_secrets": true
        }))
        .send()
        .await
        .expect("Failed to resubmit marketplace template");
    assert_eq!(StatusCode::OK, resubmit_response.status());

    let detail_response = client
        .get(format!(
            "{}/api/admin/templates/{}",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .send()
        .await
        .expect("Failed to fetch admin template detail");
    assert_eq!(StatusCode::OK, detail_response.status());

    let detail_body: Value = detail_response
        .json()
        .await
        .expect("Admin detail response should be valid JSON");
    assert_eq!(
        json!("Admin Contract Mirror Template v2"),
        detail_body["item"]["name"]
    );
    assert_eq!(
        json!("Updated short description"),
        detail_body["item"]["short_description"]
    );
    assert_eq!(
        json!("Updated long description"),
        detail_body["item"]["long_description"]
    );
    assert_eq!(json!("developer-tools"), detail_body["item"]["category_code"]);
    assert_eq!(json!(29.0), detail_body["item"]["price"]);
    assert_eq!(json!("subscription"), detail_body["item"]["billing_cycle"]);
    assert_eq!(json!("professional"), detail_body["item"]["required_plan_name"]);
    assert_eq!(json!("USD"), detail_body["item"]["currency"]);
    let versions = detail_body["item"]["versions"]
        .as_array()
        .expect("Admin detail should include template versions");
    assert_eq!(2, versions.len());

    let latest_version = versions
        .iter()
        .find(|version| version["version"] == "1.1.0")
        .expect("Expected to find the resubmitted version");
    assert_eq!(
        json!([
            {
                "path": "/etc/app/config.yaml",
                "content": "managedUpdates: true\nworker: enabled",
                "encoding": "utf-8",
                "mode": "0644"
            }
        ]),
        latest_version["config_files"]
    );
    assert_eq!(
        json!([
            {
                "name": "seed-v2",
                "command": ["bin/seed-v2.sh", "--force"],
                "timeout_seconds": 240,
                "retry_limit": 1
            }
        ]),
        latest_version["seed_jobs"]
    );
    assert_eq!(
        json!([
            {
                "name": "warm-cache-v2",
                "command": "bin/warm-cache-v2.sh",
                "execution_scope": "container",
                "timeout_seconds": 90
            }
        ]),
        latest_version["post_deploy_hooks"]
    );
    assert_eq!(
        json!([
            {
                "storage_provider": "hetzner-object-storage",
                "bucket": "marketplace-assets-test",
                "key": "templates/admin-contract-mirror-template/versions/1.1.0/assets/def67890/bundle-v2.tgz",
                "filename": "bundle-v2.tgz",
                "sha256": "def67890",
                "size": 1024,
                "content_type": "application/gzip",
                "decompress": true
            }
        ]),
        latest_version["assets"]
    );
    assert_eq!(
        json!({
            "mode_self_managed": true,
            "mode_managed_status_panel": true,
            "supports_rollback": true,
            "requires_backup": true,
            "backup_providers": ["snapshot"]
        }),
        latest_version["update_mode_capabilities"]
    );
    assert_eq!(
        "Adds worker seed and managed status panel updates.",
        latest_version["changelog"]
    );

    let initial_version = versions
        .iter()
        .find(|version| version["version"] == "1.0.0")
        .expect("Expected to find the initial version");
    assert_eq!(
        json!([
            {
                "path": "/etc/app/config.yaml",
                "content": "managedUpdates: true",
                "encoding": "utf-8",
                "mode": "0644"
            }
        ]),
        initial_version["config_files"]
    );
    assert_eq!(
        json!({
            "mode_self_managed": true,
            "mode_managed_status_panel": false,
            "supports_rollback": true,
            "requires_backup": false,
            "backup_providers": []
        }),
        initial_version["update_mode_capabilities"]
    );
}

#[tokio::test]
async fn resubmit_same_version_updates_latest_version_in_place() {
    let app = match common::spawn_app().await {
        Some(app) => app,
        None => return,
    };
    let client = reqwest::Client::new();

    let create_response = client
        .post(format!("{}/api/templates", app.address))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({
            "name": "Same Version Resubmit Template",
            "slug": "same-version-resubmit-template",
            "version": "1.0.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.27" }
                }
            },
            "changelog": "Initial version"
        }))
        .send()
        .await
        .expect("Failed to create marketplace template");
    assert_eq!(StatusCode::CREATED, create_response.status());

    let create_body: Value = create_response
        .json()
        .await
        .expect("Create response should be valid JSON");
    let template_id = create_body["item"]["id"]
        .as_str()
        .expect("Created template should include an id")
        .to_string();
    let template_uuid = uuid::Uuid::parse_str(&template_id).expect("Template id should be a UUID");

    sqlx::query(
        r#"UPDATE stack_template SET status = 'approved', approved_at = NOW() WHERE id = $1"#,
    )
    .bind(template_uuid)
    .execute(&app.db_pool)
    .await
    .expect("Failed to approve template for resubmission");

    let resubmit_response = client
        .post(format!(
            "{}/api/templates/{}/resubmit",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", common::USER_A_TOKEN))
        .json(&json!({
            "version": "1.0.0",
            "stack_definition": {
                "services": {
                    "web": { "image": "nginx:1.28" },
                    "worker": { "image": "alpine:3.20" }
                }
            },
            "changelog": "Updated same version after review feedback",
            "confirm_no_secrets": true
        }))
        .send()
        .await
        .expect("Failed to resubmit marketplace template");
    assert_eq!(StatusCode::OK, resubmit_response.status());

    let version_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM stack_template_version WHERE template_id = $1"#,
    )
    .bind(template_uuid)
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to count template versions");
    assert_eq!(1, version_count, "Same-version resubmit should update in place");

    let detail_response = client
        .get(format!(
            "{}/api/admin/templates/{}",
            app.address, template_id
        ))
        .header("Authorization", format!("Bearer {}", create_admin_jwt()))
        .send()
        .await
        .expect("Failed to fetch admin template detail");
    assert_eq!(StatusCode::OK, detail_response.status());
    let detail_body: Value = detail_response
        .json()
        .await
        .expect("Admin detail response should be valid JSON");
    let versions = detail_body["item"]["versions"]
        .as_array()
        .expect("Admin detail should include template versions");
    assert_eq!(1, versions.len());
    assert_eq!(
        json!("Updated same version after review feedback"),
        versions[0]["changelog"]
    );
    assert_eq!(
        json!({
            "services": {
                "web": { "image": "nginx:1.28" },
                "worker": { "image": "alpine:3.20" }
            }
        }),
        versions[0]["stack_definition"]
    );
}
