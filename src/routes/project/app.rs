//! REST API routes for app configuration management.
//!
//! Endpoints for managing app configurations within projects:
//! - POST /project/{project_id}/apps - Create or update an app in a project
//! - GET /project/{project_id}/apps - List all apps in a project
//! - GET /project/{project_id}/apps/{code} - Get a specific app
//! - GET /project/{project_id}/apps/{code}/config - Get app configuration
//! - PUT /project/{project_id}/apps/{code}/config - Update app configuration
//! - GET /project/{project_id}/apps/{code}/env - Get environment variables
//! - PUT /project/{project_id}/apps/{code}/env - Update environment variables
//! - DELETE /project/{project_id}/apps/{code}/env/{name} - Delete environment variable
//! - PUT /project/{project_id}/apps/{code}/ports - Update port mappings
//! - PUT /project/{project_id}/apps/{code}/domain - Update domain settings

use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{delete, get, post, put, web, Responder, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;

use crate::services::ProjectAppService;

/// Response for app configuration
#[derive(Debug, Serialize)]
pub struct AppConfigResponse {
    pub project_id: i32,
    pub app_code: String,
    pub environment: Value,
    pub ports: Value,
    pub volumes: Value,
    pub domain: Option<String>,
    pub ssl_enabled: bool,
    pub resources: Value,
    pub restart_policy: String,
}

/// Request to update environment variables
#[derive(Debug, Deserialize)]
pub struct UpdateEnvRequest {
    pub variables: Value, // JSON object of key-value pairs
}

/// Request to update a single environment variable
#[derive(Debug, Deserialize)]
pub struct SetEnvVarRequest {
    pub name: String,
    pub value: String,
}

/// Request to update port mappings
#[derive(Debug, Deserialize)]
pub struct UpdatePortsRequest {
    pub ports: Vec<PortMapping>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PortMapping {
    pub host: u16,
    pub container: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_protocol() -> String {
    "tcp".to_string()
}

/// Request to update domain settings
#[derive(Debug, Deserialize)]
pub struct UpdateDomainRequest {
    pub domain: Option<String>,
    #[serde(default)]
    pub ssl_enabled: bool,
}

/// Request to create or update an app in a project
#[derive(Debug, Deserialize)]
pub struct CreateAppRequest {
    #[serde(alias = "app_code")]
    pub code: String,
    #[serde(default)]
    pub name: Option<String>,
    pub image: String,
    #[serde(default, alias = "environment")]
    pub env: Option<Value>,
    #[serde(default)]
    pub ports: Option<Value>,
    #[serde(default)]
    pub volumes: Option<Value>,
    #[serde(default)]
    pub config_files: Option<Value>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub ssl_enabled: Option<bool>,
    #[serde(default)]
    pub resources: Option<Value>,
    #[serde(default)]
    pub restart_policy: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub networks: Option<Value>,
    #[serde(default)]
    pub depends_on: Option<Value>,
    #[serde(default)]
    pub healthcheck: Option<Value>,
    #[serde(default)]
    pub labels: Option<Value>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub deploy_order: Option<i32>,
    #[serde(default)]
    pub deployment_hash: Option<String>,
}

/// List all apps in a project
#[tracing::instrument(name = "List project apps", skip(pg_pool))]
#[get("/{project_id}/apps")]
pub async fn list_apps(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let project_id = path.0;

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    // Fetch apps for project
    let apps = db::project_app::fetch_by_project(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?;

    Ok(JsonResponse::build().set_list(apps).ok("OK"))
}

/// Create or update an app in a project
#[tracing::instrument(name = "Create project app", skip(pg_pool))]
#[post("/{project_id}/apps")]
pub async fn create_app(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    payload: web::Json<CreateAppRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let project_id = path.0;

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    let code = payload.code.trim();
    if code.is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("app code is required"));
    }

    let image = payload.image.trim();
    if image.is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("image is required"));
    }

    let mut app = models::ProjectApp::default();
    app.project_id = project_id;
    app.code = code.to_string();
    app.name = payload
        .name
        .clone()
        .unwrap_or_else(|| code.to_string());
    app.image = image.to_string();
    app.environment = payload.env.clone();
    app.ports = payload.ports.clone();
    app.volumes = payload.volumes.clone();
    app.domain = payload.domain.clone();
    app.ssl_enabled = payload.ssl_enabled;
    app.resources = payload.resources.clone();
    app.restart_policy = payload.restart_policy.clone();
    app.command = payload.command.clone();
    app.entrypoint = payload.entrypoint.clone();
    app.networks = payload.networks.clone();
    app.depends_on = payload.depends_on.clone();
    app.healthcheck = payload.healthcheck.clone();
    app.labels = payload.labels.clone();
    app.enabled = payload.enabled.or(Some(true));
    app.deploy_order = payload.deploy_order;

    if let Some(config_files) = payload.config_files.clone() {
        let mut labels = app.labels.clone().unwrap_or(json!({}));
        if let Some(obj) = labels.as_object_mut() {
            obj.insert("config_files".to_string(), config_files);
        }
        app.labels = Some(labels);
    }

    let app_service = if let Some(deployment_hash) = payload.deployment_hash.as_deref() {
        let service = ProjectAppService::new(Arc::new(pg_pool.get_ref().clone()))
            .map_err(|e| JsonResponse::<()>::build().internal_server_error(e))?;
        let created = service
            .upsert(&app, &project, deployment_hash)
            .await
            .map_err(|e| JsonResponse::<()>::build().internal_server_error(e.to_string()))?;
        return Ok(JsonResponse::build().set_item(Some(created)).ok("OK"));
    } else {
        ProjectAppService::new_without_sync(Arc::new(pg_pool.get_ref().clone()))
            .map_err(|e| JsonResponse::<()>::build().internal_server_error(e))?
    };

    let created = app_service
        .upsert(&app, &project, "")
        .await
        .map_err(|e| JsonResponse::<()>::build().internal_server_error(e.to_string()))?;

    Ok(JsonResponse::build().set_item(Some(created)).ok("OK"))
}

/// Get a specific app by code
#[tracing::instrument(name = "Get project app", skip(pg_pool))]
#[get("/{project_id}/apps/{code}")]
pub async fn get_app(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32, String)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project_id, code) = path.into_inner();

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    // Fetch app
    let app = db::project_app::fetch_by_project_and_code(pg_pool.get_ref(), project_id, &code)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("App not found"))?;

    Ok(JsonResponse::build().set_item(Some(app)).ok("OK"))
}

/// Get app configuration (env vars, ports, domain, etc.)
#[tracing::instrument(name = "Get app config", skip(pg_pool))]
#[get("/{project_id}/apps/{code}/config")]
pub async fn get_app_config(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32, String)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project_id, code) = path.into_inner();

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    // Fetch app
    let app = db::project_app::fetch_by_project_and_code(pg_pool.get_ref(), project_id, &code)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("App not found"))?;

    // Build response with redacted environment variables
    let env = redact_sensitive_env_vars(app.environment.clone().unwrap_or(json!({})));

    let config = AppConfigResponse {
        project_id,
        app_code: code,
        environment: env,
        ports: app.ports.clone().unwrap_or(json!([])),
        volumes: app.volumes.clone().unwrap_or(json!([])),
        domain: app.domain.clone(),
        ssl_enabled: app.ssl_enabled.unwrap_or(false),
        resources: app.resources.clone().unwrap_or(json!({})),
        restart_policy: app
            .restart_policy
            .clone()
            .unwrap_or("unless-stopped".to_string()),
    };

    Ok(JsonResponse::build().set_item(Some(config)).ok("OK"))
}

/// Get environment variables for an app
#[tracing::instrument(name = "Get app env vars", skip(pg_pool))]
#[get("/{project_id}/apps/{code}/env")]
pub async fn get_env_vars(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32, String)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project_id, code) = path.into_inner();

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    // Fetch app
    let app = db::project_app::fetch_by_project_and_code(pg_pool.get_ref(), project_id, &code)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("App not found"))?;

    // Redact sensitive values
    let env = redact_sensitive_env_vars(app.environment.clone().unwrap_or(json!({})));

    let response = json!({
        "project_id": project_id,
        "app_code": code,
        "variables": env,
        "count": env.as_object().map(|o| o.len()).unwrap_or(0),
        "note": "Sensitive values (passwords, tokens, keys) are redacted"
    });

    Ok(JsonResponse::build().set_item(Some(response)).ok("OK"))
}

/// Update environment variables for an app
#[tracing::instrument(name = "Update app env vars", skip(pg_pool, body))]
#[put("/{project_id}/apps/{code}/env")]
pub async fn update_env_vars(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32, String)>,
    body: web::Json<UpdateEnvRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project_id, code) = path.into_inner();

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    // Fetch and update app
    let mut app = db::project_app::fetch_by_project_and_code(pg_pool.get_ref(), project_id, &code)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("App not found"))?;

    // Merge new variables with existing
    let mut env = app.environment.clone().unwrap_or(json!({}));
    if let (Some(existing), Some(new)) = (env.as_object_mut(), body.variables.as_object()) {
        for (key, value) in new {
            existing.insert(key.clone(), value.clone());
        }
    }
    app.environment = Some(env);

    // Save
    let updated = db::project_app::update(pg_pool.get_ref(), &app)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?;

    tracing::info!(
        user_id = %user.id,
        project_id = project_id,
        app_code = %code,
        "Updated environment variables"
    );

    Ok(JsonResponse::build()
        .set_item(Some(json!({
            "success": true,
            "message": "Environment variables updated. Changes will take effect on next restart.",
            "updated_at": updated.updated_at
        })))
        .ok("OK"))
}

/// Delete a specific environment variable
#[tracing::instrument(name = "Delete app env var", skip(pg_pool))]
#[delete("/{project_id}/apps/{code}/env/{name}")]
pub async fn delete_env_var(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32, String, String)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project_id, code, var_name) = path.into_inner();

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    // Fetch and update app
    let mut app = db::project_app::fetch_by_project_and_code(pg_pool.get_ref(), project_id, &code)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("App not found"))?;

    // Remove the variable
    let mut env = app.environment.clone().unwrap_or(json!({}));
    let existed = if let Some(obj) = env.as_object_mut() {
        obj.remove(&var_name).is_some()
    } else {
        false
    };
    app.environment = Some(env);

    if !existed {
        return Err(JsonResponse::not_found("Environment variable not found"));
    }

    // Save
    db::project_app::update(pg_pool.get_ref(), &app)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?;

    tracing::info!(
        user_id = %user.id,
        project_id = project_id,
        app_code = %code,
        var_name = %var_name,
        "Deleted environment variable"
    );

    Ok(JsonResponse::build()
        .set_item(Some(json!({
            "success": true,
            "message": format!("Environment variable '{}' deleted", var_name)
        })))
        .ok("OK"))
}

/// Update port mappings for an app
#[tracing::instrument(name = "Update app ports", skip(pg_pool, body))]
#[put("/{project_id}/apps/{code}/ports")]
pub async fn update_ports(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32, String)>,
    body: web::Json<UpdatePortsRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project_id, code) = path.into_inner();

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    // Fetch and update app
    let mut app = db::project_app::fetch_by_project_and_code(pg_pool.get_ref(), project_id, &code)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("App not found"))?;

    // Update ports
    app.ports = Some(serde_json::to_value(&body.ports).unwrap_or(json!([])));

    // Save
    let updated = db::project_app::update(pg_pool.get_ref(), &app)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?;

    tracing::info!(
        user_id = %user.id,
        project_id = project_id,
        app_code = %code,
        port_count = body.ports.len(),
        "Updated port mappings"
    );

    Ok(JsonResponse::build()
        .set_item(Some(json!({
            "success": true,
            "message": "Port mappings updated. Changes will take effect on next restart.",
            "ports": updated.ports,
            "updated_at": updated.updated_at
        })))
        .ok("OK"))
}

/// Update domain and SSL settings for an app
#[tracing::instrument(name = "Update app domain", skip(pg_pool, body))]
#[put("/{project_id}/apps/{code}/domain")]
pub async fn update_domain(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32, String)>,
    body: web::Json<UpdateDomainRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project_id, code) = path.into_inner();

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    // Fetch and update app
    let mut app = db::project_app::fetch_by_project_and_code(pg_pool.get_ref(), project_id, &code)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("App not found"))?;

    // Update domain settings
    app.domain = body.domain.clone();
    app.ssl_enabled = Some(body.ssl_enabled);

    // Save
    let updated = db::project_app::update(pg_pool.get_ref(), &app)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?;

    tracing::info!(
        user_id = %user.id,
        project_id = project_id,
        app_code = %code,
        domain = ?body.domain,
        ssl_enabled = body.ssl_enabled,
        "Updated domain settings"
    );

    Ok(JsonResponse::build()
        .set_item(Some(json!({
            "success": true,
            "message": "Domain settings updated. Changes will take effect on next restart.",
            "domain": updated.domain,
            "ssl_enabled": updated.ssl_enabled,
            "updated_at": updated.updated_at
        })))
        .ok("OK"))
}

/// Redact sensitive environment variables for display
fn redact_sensitive_env_vars(env: Value) -> Value {
    const SENSITIVE_PATTERNS: &[&str] = &[
        "password",
        "passwd",
        "secret",
        "token",
        "key",
        "api_key",
        "apikey",
        "auth",
        "credential",
        "private",
        "cert",
        "ssl",
        "tls",
    ];

    if let Some(obj) = env.as_object() {
        let redacted: serde_json::Map<String, Value> = obj
            .iter()
            .map(|(k, v)| {
                let key_lower = k.to_lowercase();
                let is_sensitive = SENSITIVE_PATTERNS.iter().any(|p| key_lower.contains(p));
                if is_sensitive {
                    (k.clone(), json!("[REDACTED]"))
                } else {
                    (k.clone(), v.clone())
                }
            })
            .collect();
        Value::Object(redacted)
    } else {
        env
    }
}
