mod common;

use reqwest::StatusCode;
use serde_json::json;
use sqlx::Row;
use tokio::sync::Mutex;
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

static APP_SERVICE_ENV_LOCK: Mutex<()> = Mutex::const_new(());

async fn mount_server_catalog(
    provider: &str,
    cloud_id: Option<i32>,
    servers: serde_json::Value,
) -> MockServer {
    let server = MockServer::start().await;
    let mut mock = Mock::given(method("GET")).and(path(format!("/{}/servers", provider)));

    if let Some(cloud_id) = cloud_id {
        mock = mock.and(query_param("cloud_id", cloud_id.to_string()));
    }

    mock.respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "_status": "OK",
        "servers": servers
    })))
    .mount(&server)
    .await;

    server
}

async fn create_marketplace_project(
    pool: &sqlx::PgPool,
    user_id: &str,
    infrastructure_requirements: serde_json::Value,
) -> i32 {
    let project_id = sqlx::query(
        r#"INSERT INTO project (
            stack_id,
            user_id,
            name,
            metadata,
            request_json,
            created_at,
            updated_at
        )
        VALUES (gen_random_uuid(), $1, 'Test Project', '{}'::jsonb, '{}'::jsonb, NOW(), NOW())
        RETURNING id"#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("Failed to insert test project")
    .get::<i32, _>("id");
    let slug = format!("deploy-validation-{}", Uuid::new_v4());
    let template_name = format!("Deploy Validation {}", Uuid::new_v4());

    let template_id = sqlx::query(
        r#"INSERT INTO stack_template (
            creator_user_id,
            name,
            slug,
            status,
            tags,
            tech_stack,
            infrastructure_requirements
        )
        VALUES ($1, $2, $3, 'approved', '[]'::jsonb, '{}'::jsonb, $4)
        RETURNING id"#,
    )
    .bind(user_id)
    .bind(template_name)
    .bind(slug)
    .bind(infrastructure_requirements)
    .fetch_one(pool)
    .await
    .expect("Failed to insert marketplace template")
    .get::<Uuid, _>("id");

    sqlx::query("UPDATE project SET source_template_id = $1 WHERE id = $2")
        .bind(template_id)
        .bind(project_id)
        .execute(pool)
        .await
        .expect("Failed to attach template to project");

    project_id
}

#[tokio::test]
async fn deploy_rejects_marketplace_targets_that_do_not_match_template_requirements() {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = create_marketplace_project(
        &app.db_pool,
        "test_user_id",
        json!({
            "supported_clouds": ["htz"],
            "supported_os": ["ubuntu-22.04"]
        }),
    )
    .await;

    let response = reqwest::Client::new()
        .post(format!("{}/project/{}/deploy", app.address, project_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "stack": {
                "vars": [],
                "integrated_features": [],
                "extended_features": [],
                "subscriptions": [],
                "form_app": []
            },
            "cloud": {
                "provider": "aws",
                "cloud_token": "test-cloud-token",
                "save_token": false
            },
            "server": {
                "region": "us-east-1",
                "server": "t3.small",
                "os": "ubuntu-24.04",
                "disk_type": "gp3"
            }
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("response body should be json");
    let message = body["message"]
        .as_str()
        .expect("error response should include message");

    assert!(message.contains("htz"));
    assert!(message.contains("ubuntu-22.04"));
}

#[tokio::test]
async fn deploy_with_saved_cloud_rejects_marketplace_targets_that_do_not_match_template_requirements(
) {
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = create_marketplace_project(
        &app.db_pool,
        "test_user_id",
        json!({
            "supported_clouds": ["htz"],
            "supported_os": ["ubuntu-22.04"]
        }),
    )
    .await;
    let cloud_id =
        common::create_test_cloud(&app.db_pool, "test_user_id", "saved-aws", "aws").await;

    let response = reqwest::Client::new()
        .post(format!(
            "{}/project/{}/deploy/{}",
            app.address, project_id, cloud_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "stack": {
                "vars": [],
                "integrated_features": [],
                "extended_features": [],
                "subscriptions": [],
                "form_app": []
            },
            "cloud": {
                "provider": "aws"
            },
            "server": {
                "region": "us-east-1",
                "server": "t3.small",
                "os": "ubuntu-24.04",
                "disk_type": "gp3"
            }
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("response body should be json");
    let message = body["message"]
        .as_str()
        .expect("error response should include message");

    assert!(message.contains("htz"));
    assert!(message.contains("ubuntu-22.04"));
}

#[tokio::test]
async fn deploy_rejects_marketplace_targets_that_do_not_meet_min_ram_mb_requirement() {
    let _guard = APP_SERVICE_ENV_LOCK.lock().await;
    let app_service = mount_server_catalog(
        "aws",
        None,
        json!([
            {
                "id": "t3.small",
                "ram": 1,
                "vcpu": 2,
                "disk_size": 20
            },
            {
                "id": "t3.medium",
                "ram": 2,
                "vcpu": 2,
                "disk_size": 40
            }
        ]),
    )
    .await;
    std::env::set_var("APP_SERVICE_URL", app_service.uri());

    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = create_marketplace_project(
        &app.db_pool,
        "test_user_id",
        json!({
            "supported_clouds": ["aws"],
            "supported_os": ["ubuntu-24.04"],
            "min_ram_mb": 2048
        }),
    )
    .await;

    let response = reqwest::Client::new()
        .post(format!("{}/project/{}/deploy", app.address, project_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "stack": {
                "vars": [],
                "integrated_features": [],
                "extended_features": [],
                "subscriptions": [],
                "form_app": []
            },
            "cloud": {
                "provider": "aws",
                "cloud_token": "test-cloud-token",
                "save_token": false
            },
            "server": {
                "region": "us-east-1",
                "server": "t3.small",
                "os": "ubuntu-24.04",
                "disk_type": "gp3"
            }
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("response body should be json");
    let message = body["message"]
        .as_str()
        .expect("error response should include message");

    assert!(message.contains("minimum RAM"));
    assert!(message.contains("2048"));
    assert!(message.contains("1024"));
}

#[tokio::test]
async fn deploy_with_saved_cloud_rejects_marketplace_targets_that_do_not_meet_min_ram_mb_requirement(
) {
    let _guard = APP_SERVICE_ENV_LOCK.lock().await;
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = create_marketplace_project(
        &app.db_pool,
        "test_user_id",
        json!({
            "supported_clouds": ["aws"],
            "supported_os": ["ubuntu-24.04"],
            "min_ram_mb": 2048
        }),
    )
    .await;
    let cloud_id =
        common::create_test_cloud(&app.db_pool, "test_user_id", "saved-aws", "aws").await;
    let app_service = mount_server_catalog(
        "aws",
        Some(cloud_id),
        json!([
            {
                "id": "t3.small",
                "ram": 1,
                "vcpu": 2,
                "disk_size": 20
            }
        ]),
    )
    .await;
    std::env::set_var("APP_SERVICE_URL", app_service.uri());

    let response = reqwest::Client::new()
        .post(format!(
            "{}/project/{}/deploy/{}",
            app.address, project_id, cloud_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "stack": {
                "vars": [],
                "integrated_features": [],
                "extended_features": [],
                "subscriptions": [],
                "form_app": []
            },
            "cloud": {
                "provider": "aws"
            },
            "server": {
                "region": "us-east-1",
                "server": "t3.small",
                "os": "ubuntu-24.04",
                "disk_type": "gp3"
            }
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("response body should be json");
    let message = body["message"]
        .as_str()
        .expect("error response should include message");

    assert!(message.contains("minimum RAM"));
    assert!(message.contains("2048"));
    assert!(message.contains("1024"));
}

#[tokio::test]
async fn deploy_rejects_marketplace_targets_that_do_not_meet_min_disk_gb_requirement() {
    let _guard = APP_SERVICE_ENV_LOCK.lock().await;
    let app_service = mount_server_catalog(
        "aws",
        None,
        json!([
            {
                "id": "t3.small",
                "ram": 2,
                "vcpu": 2,
                "disk_size": 20
            }
        ]),
    )
    .await;
    std::env::set_var("APP_SERVICE_URL", app_service.uri());

    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = create_marketplace_project(
        &app.db_pool,
        "test_user_id",
        json!({
            "supported_clouds": ["aws"],
            "supported_os": ["ubuntu-24.04"],
            "min_disk_gb": 40
        }),
    )
    .await;

    let response = reqwest::Client::new()
        .post(format!("{}/project/{}/deploy", app.address, project_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "stack": {
                "vars": [],
                "integrated_features": [],
                "extended_features": [],
                "subscriptions": [],
                "form_app": []
            },
            "cloud": {
                "provider": "aws",
                "cloud_token": "test-cloud-token",
                "save_token": false
            },
            "server": {
                "region": "us-east-1",
                "server": "t3.small",
                "os": "ubuntu-24.04",
                "disk_type": "gp3"
            }
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("response body should be json");
    let message = body["message"]
        .as_str()
        .expect("error response should include message");

    assert!(message.contains("minimum disk"));
    assert!(message.contains("40"));
    assert!(message.contains("20"));
}

#[tokio::test]
async fn deploy_with_saved_cloud_rejects_marketplace_targets_that_do_not_meet_min_disk_gb_requirement(
) {
    let _guard = APP_SERVICE_ENV_LOCK.lock().await;
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = create_marketplace_project(
        &app.db_pool,
        "test_user_id",
        json!({
            "supported_clouds": ["aws"],
            "supported_os": ["ubuntu-24.04"],
            "min_disk_gb": 40
        }),
    )
    .await;
    let cloud_id =
        common::create_test_cloud(&app.db_pool, "test_user_id", "saved-aws", "aws").await;
    let app_service = mount_server_catalog(
        "aws",
        Some(cloud_id),
        json!([
            {
                "id": "t3.small",
                "ram": 2,
                "vcpu": 2,
                "disk_size": 20
            }
        ]),
    )
    .await;
    std::env::set_var("APP_SERVICE_URL", app_service.uri());

    let response = reqwest::Client::new()
        .post(format!(
            "{}/project/{}/deploy/{}",
            app.address, project_id, cloud_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "stack": {
                "vars": [],
                "integrated_features": [],
                "extended_features": [],
                "subscriptions": [],
                "form_app": []
            },
            "cloud": {
                "provider": "aws"
            },
            "server": {
                "region": "us-east-1",
                "server": "t3.small",
                "os": "ubuntu-24.04",
                "disk_type": "gp3"
            }
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("response body should be json");
    let message = body["message"]
        .as_str()
        .expect("error response should include message");

    assert!(message.contains("minimum disk"));
    assert!(message.contains("40"));
    assert!(message.contains("20"));
}

#[tokio::test]
async fn deploy_rejects_marketplace_targets_that_do_not_meet_min_cpu_cores_requirement() {
    let _guard = APP_SERVICE_ENV_LOCK.lock().await;
    let app_service = mount_server_catalog(
        "aws",
        None,
        json!([
            {
                "id": "t3.small",
                "ram": 4,
                "vcpu": 2,
                "disk_size": 80
            }
        ]),
    )
    .await;
    std::env::set_var("APP_SERVICE_URL", app_service.uri());

    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = create_marketplace_project(
        &app.db_pool,
        "test_user_id",
        json!({
            "supported_clouds": ["aws"],
            "supported_os": ["ubuntu-24.04"],
            "min_cpu_cores": 4
        }),
    )
    .await;

    let response = reqwest::Client::new()
        .post(format!("{}/project/{}/deploy", app.address, project_id))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "stack": {
                "vars": [],
                "integrated_features": [],
                "extended_features": [],
                "subscriptions": [],
                "form_app": []
            },
            "cloud": {
                "provider": "aws",
                "cloud_token": "test-cloud-token",
                "save_token": false
            },
            "server": {
                "region": "us-east-1",
                "server": "t3.small",
                "os": "ubuntu-24.04",
                "disk_type": "gp3"
            }
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("response body should be json");
    let message = body["message"]
        .as_str()
        .expect("error response should include message");

    assert!(message.contains("minimum CPU"));
    assert!(message.contains("4"));
    assert!(message.contains("2"));
}

#[tokio::test]
async fn deploy_with_saved_cloud_rejects_marketplace_targets_that_do_not_meet_min_cpu_cores_requirement(
) {
    let _guard = APP_SERVICE_ENV_LOCK.lock().await;
    let Some(app) = common::spawn_app_with_vault().await else {
        return;
    };

    let project_id = create_marketplace_project(
        &app.db_pool,
        "test_user_id",
        json!({
            "supported_clouds": ["aws"],
            "supported_os": ["ubuntu-24.04"],
            "min_cpu_cores": 4
        }),
    )
    .await;
    let cloud_id =
        common::create_test_cloud(&app.db_pool, "test_user_id", "saved-aws", "aws").await;
    let app_service = mount_server_catalog(
        "aws",
        Some(cloud_id),
        json!([
            {
                "id": "t3.small",
                "ram": 4,
                "vcpu": 2,
                "disk_size": 80
            }
        ]),
    )
    .await;
    std::env::set_var("APP_SERVICE_URL", app_service.uri());

    let response = reqwest::Client::new()
        .post(format!(
            "{}/project/{}/deploy/{}",
            app.address, project_id, cloud_id
        ))
        .bearer_auth("test-bearer-token")
        .json(&json!({
            "stack": {
                "vars": [],
                "integrated_features": [],
                "extended_features": [],
                "subscriptions": [],
                "form_app": []
            },
            "cloud": {
                "provider": "aws"
            },
            "server": {
                "region": "us-east-1",
                "server": "t3.small",
                "os": "ubuntu-24.04",
                "disk_type": "gp3"
            }
        }))
        .send()
        .await
        .expect("deploy request failed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("response body should be json");
    let message = body["message"]
        .as_str()
        .expect("error response should include message");

    assert!(message.contains("minimum CPU"));
    assert!(message.contains("4"));
    assert!(message.contains("2"));
}
