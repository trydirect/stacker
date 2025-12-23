use crate::db;
use crate::helpers::JsonResponse;
use crate::models::{Command, CommandPriority, User};
use actix_web::{post, web, Responder, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct CreateCommandRequest {
    pub deployment_hash: String,
    pub command_type: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
    #[serde(default)]
    pub timeout_seconds: Option<i32>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Default)]
pub struct CreateCommandResponse {
    pub command_id: String,
    pub deployment_hash: String,
    pub status: String,
}

#[tracing::instrument(name = "Create command", skip(pg_pool, user))]
#[post("")]
pub async fn create_handler(
    user: web::ReqData<Arc<User>>,
    req: web::Json<CreateCommandRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    // Generate unique command ID
    let command_id = format!("cmd_{}", uuid::Uuid::new_v4());

    // Parse priority or default to Normal
    let priority = req
        .priority
        .as_ref()
        .and_then(|p| match p.to_lowercase().as_str() {
            "low" => Some(CommandPriority::Low),
            "normal" => Some(CommandPriority::Normal),
            "high" => Some(CommandPriority::High),
            "critical" => Some(CommandPriority::Critical),
            _ => None,
        })
        .unwrap_or(CommandPriority::Normal);

    // Build command
    let mut command = Command::new(
        command_id.clone(),
        req.deployment_hash.clone(),
        req.command_type.clone(),
        user.id.clone(),
    )
    .with_priority(priority.clone());

    if let Some(params) = &req.parameters {
        command = command.with_parameters(params.clone());
    }

    if let Some(timeout) = req.timeout_seconds {
        command = command.with_timeout(timeout);
    }

    if let Some(metadata) = &req.metadata {
        command = command.with_metadata(metadata.clone());
    }

    // Insert command into database
    let saved_command = db::command::insert(pg_pool.get_ref(), &command)
        .await
        .map_err(|err| {
            tracing::error!("Failed to create command: {}", err);
            JsonResponse::<()>::build().internal_server_error(err)
        })?;

    // Add to queue
    db::command::add_to_queue(
        pg_pool.get_ref(),
        &saved_command.command_id,
        &saved_command.deployment_hash,
        &priority,
    )
    .await
    .map_err(|err| {
        tracing::error!("Failed to add command to queue: {}", err);
        JsonResponse::<()>::build().internal_server_error(err)
    })?;

    tracing::info!(
        "Command created: {} for deployment {}",
        saved_command.command_id,
        saved_command.deployment_hash
    );

    let response = CreateCommandResponse {
        command_id: saved_command.command_id,
        deployment_hash: saved_command.deployment_hash,
        status: saved_command.status,
    };

    Ok(JsonResponse::build()
        .set_item(Some(response))
        .created("Command created successfully"))
}
