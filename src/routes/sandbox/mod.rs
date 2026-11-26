//! Sandbox exec/file REST API (WS-S S2). Owner-gated; runs commands and file
//! operations inside a sandbox via the agent command queue.

use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use crate::services::sandbox as sandbox_service;
use actix_web::{delete, get, post, web, Responder, Result};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Load a sandbox and verify the caller owns it. Non-owner answers 404 (same
/// as missing) so the response does not reveal another user's sandbox.
async fn fetch_owned_sandbox(
    pool: &PgPool,
    user: &models::User,
    id: Uuid,
) -> Result<models::Sandbox, actix_web::Error> {
    let sandbox = db::sandbox::fetch(pool, id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Sandbox not found"))?;
    if sandbox.user_id != user.id {
        return Err(JsonResponse::not_found("Sandbox not found"));
    }
    Ok(sandbox)
}

/// The sandbox must have a deployment to address the agent.
fn deployment_id(sandbox: &models::Sandbox) -> Result<i32, actix_web::Error> {
    sandbox
        .deployment_id
        .ok_or_else(|| JsonResponse::bad_request("Sandbox has no target deployment"))
}

#[derive(Deserialize)]
pub struct ExecRequest {
    pub command: String,
    #[serde(rename = "timeoutSec")]
    pub timeout_sec: Option<u32>,
}

#[post("/{id}/exec")]
pub async fn exec_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    body: web::Json<ExecRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();
    let sandbox = fetch_owned_sandbox(pg_pool.get_ref(), &user, id).await?;
    let deployment_id = deployment_id(&sandbox)?;

    let result = sandbox_service::exec(
        pg_pool.get_ref(),
        id,
        deployment_id,
        &body.command,
        body.timeout_sec,
    )
    .await
    .map_err(JsonResponse::internal_server_error)?;

    Ok(JsonResponse::build().set_item(result).ok("Exec complete"))
}

#[derive(Deserialize)]
pub struct WriteFileRequest {
    pub path: String,
    #[serde(rename = "contentBase64")]
    pub content_base64: String,
}

#[post("/{id}/files")]
pub async fn write_file_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    body: web::Json<WriteFileRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();
    let sandbox = fetch_owned_sandbox(pg_pool.get_ref(), &user, id).await?;
    let deployment_id = deployment_id(&sandbox)?;

    if body.content_base64.len() > sandbox_service::MAX_FILE_BYTES * 4 / 3 + 4 {
        return Err(JsonResponse::bad_request("file exceeds the 10 MiB cap"));
    }

    let result = sandbox_service::write_file(
        pg_pool.get_ref(),
        id,
        deployment_id,
        &body.path,
        &body.content_base64,
    )
    .await
    .map_err(JsonResponse::internal_server_error)?;

    Ok(JsonResponse::build().set_item(result).ok("File written"))
}

#[derive(Deserialize)]
pub struct FilePathQuery {
    pub path: String,
}

#[get("/{id}/files")]
pub async fn read_file_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    query: web::Query<FilePathQuery>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();
    let sandbox = fetch_owned_sandbox(pg_pool.get_ref(), &user, id).await?;
    let deployment_id = deployment_id(&sandbox)?;

    let result = sandbox_service::read_file(pg_pool.get_ref(), id, deployment_id, &query.path)
        .await
        .map_err(JsonResponse::internal_server_error)?;

    Ok(JsonResponse::build().set_item(result).ok("File read"))
}

#[get("/{id}/files/list")]
pub async fn list_files_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    query: web::Query<FilePathQuery>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();
    let sandbox = fetch_owned_sandbox(pg_pool.get_ref(), &user, id).await?;
    let deployment_id = deployment_id(&sandbox)?;

    let result = sandbox_service::list_files(pg_pool.get_ref(), id, deployment_id, &query.path)
        .await
        .map_err(JsonResponse::internal_server_error)?;

    Ok(JsonResponse::build()
        .set_item(result)
        .ok("Directory listing"))
}

/// Create/list/get/kill (S1 lifecycle surface). Create enqueues a `sandbox`
/// create for the agent; kill enqueues a `sandbox` kill and marks the row.
#[derive(Deserialize)]
pub struct CreateSandboxBody {
    #[serde(rename = "projectId")]
    pub project_id: Option<i32>,
    pub image: String,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(rename = "networkMode")]
    pub network_mode: Option<String>,
    #[serde(default)]
    pub cpu: Option<f64>,
    #[serde(rename = "memMb")]
    pub mem_mb: Option<i32>,
    #[serde(rename = "pidsLimit")]
    pub pids_limit: Option<i32>,
    #[serde(rename = "inactivityTimeoutSec")]
    pub inactivity_timeout_sec: Option<i32>,
    #[serde(rename = "maxLifetimeSec")]
    pub max_lifetime_sec: Option<i32>,
    /// Write-only env; encrypted at rest, never returned.
    #[serde(default)]
    pub env: Option<serde_json::Value>,
}

fn sandbox_response(s: &models::Sandbox) -> models::SandboxResponse {
    models::SandboxResponse::from(s)
}

#[post("")]
pub async fn create_handler(
    user: web::ReqData<Arc<models::User>>,
    body: web::Json<CreateSandboxBody>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    if body.image.trim().is_empty() {
        return Err(JsonResponse::bad_request("image must not be empty"));
    }
    let runtime = body.runtime.clone().unwrap_or_else(|| "runc".to_string());
    let network_mode = body
        .network_mode
        .clone()
        .unwrap_or_else(|| "isolated".to_string());

    // Address the agent via the project's deployment (server chosen at create).
    let deployment_id = match body.project_id {
        Some(project_id) => {
            let deployment = db::deployment::fetch_by_project_id(pg_pool.get_ref(), project_id)
                .await
                .map_err(JsonResponse::internal_server_error)?;
            deployment.map(|d| d.id)
        }
        None => None,
    };

    // Env is write-only: encrypt at rest, never returned by the API (S3).
    let env_encrypted = match &body.env {
        Some(env) => Some(
            crate::helpers::chat::encrypt_messages(env)
                .map_err(JsonResponse::internal_server_error)?,
        ),
        None => None,
    };

    let sandbox = db::sandbox::insert(
        pg_pool.get_ref(),
        &user.id,
        body.project_id,
        deployment_id,
        &body.image,
        &runtime,
        &network_mode,
        body.cpu.unwrap_or(1.0),
        body.mem_mb.unwrap_or(512),
        body.pids_limit.unwrap_or(128),
        body.inactivity_timeout_sec.unwrap_or(1800),
        body.max_lifetime_sec.unwrap_or(86400),
        env_encrypted.as_deref(),
    )
    .await
    .map_err(JsonResponse::internal_server_error)?;

    // Best-effort: enqueue the container create for the agent.
    if let Some(deployment_id) = deployment_id {
        let params =
            serde_json::json!({ "action": "create", "sandbox": sandbox_response(&sandbox) });
        let _ = crate::services::agent_dispatcher::AgentDispatcher::new(pg_pool.get_ref())
            .queue_command(deployment_id, "sandbox", params)
            .await;
    }

    Ok(JsonResponse::build()
        .set_item(sandbox_response(&sandbox))
        .created("Sandbox created"))
}

#[get("")]
pub async fn list_handler(
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let list = db::sandbox::list_by_user(pg_pool.get_ref(), &user.id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    let responses: Vec<models::SandboxResponse> = list.iter().map(sandbox_response).collect();
    Ok(JsonResponse::build().set_list(responses).ok("Sandboxes"))
}

#[get("/{id}")]
pub async fn get_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();
    let sandbox = fetch_owned_sandbox(pg_pool.get_ref(), &user, id).await?;
    Ok(JsonResponse::build()
        .set_item(sandbox_response(&sandbox))
        .ok("Sandbox"))
}

#[delete("/{id}")]
pub async fn kill_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<Uuid>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();
    let sandbox = fetch_owned_sandbox(pg_pool.get_ref(), &user, id).await?;
    if let Some(deployment_id) = sandbox.deployment_id {
        let params = serde_json::json!({ "action": "kill", "sandboxId": id });
        let _ = crate::services::agent_dispatcher::AgentDispatcher::new(pg_pool.get_ref())
            .queue_command(deployment_id, "sandbox", params)
            .await;
    }
    db::sandbox::update_status(pg_pool.get_ref(), id, "killed")
        .await
        .map_err(JsonResponse::internal_server_error)?;
    Ok(JsonResponse::build()
        .set_item(json!({ "deleted": true }))
        .ok("Sandbox killed"))
}
