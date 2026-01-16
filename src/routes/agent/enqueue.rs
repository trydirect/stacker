use crate::db;
use crate::forms::status_panel;
use crate::helpers::JsonResponse;
use crate::models::{Command, CommandPriority, User};
use actix_web::{post, web, Responder, Result};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct EnqueueRequest {
    pub deployment_hash: String,
    pub command_type: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
    #[serde(default)]
    pub timeout_seconds: Option<i32>,
}

#[tracing::instrument(name = "Agent enqueue command", skip(pg_pool, user))]
#[post("/commands/enqueue")]
pub async fn enqueue_handler(
    user: web::ReqData<Arc<User>>,
    payload: web::Json<EnqueueRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    if payload.deployment_hash.trim().is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("deployment_hash is required"));
    }

    if payload.command_type.trim().is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("command_type is required"));
    }

    // Validate parameters
    let validated_parameters =
        status_panel::validate_command_parameters(&payload.command_type, &payload.parameters)
            .map_err(|err| JsonResponse::<()>::build().bad_request(err))?;

    // Generate command ID
    let command_id = format!("cmd_{}", uuid::Uuid::new_v4());

    // Parse priority
    let priority = payload
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
        payload.deployment_hash.clone(),
        payload.command_type.clone(),
        user.id.clone(),
    )
    .with_priority(priority.clone());

    if let Some(params) = &validated_parameters {
        command = command.with_parameters(params.clone());
    }

    if let Some(timeout) = payload.timeout_seconds {
        command = command.with_timeout(timeout);
    }

    // Insert command
    let saved = db::command::insert(pg_pool.get_ref(), &command)
        .await
        .map_err(|err| {
            tracing::error!("Failed to insert command: {}", err);
            JsonResponse::<()>::build().internal_server_error(err)
        })?;

    // Add to queue - agent will poll and pick it up
    db::command::add_to_queue(
        pg_pool.get_ref(),
        &saved.command_id,
        &saved.deployment_hash,
        &priority,
    )
    .await
    .map_err(|err| {
        tracing::error!("Failed to add command to queue: {}", err);
        JsonResponse::<()>::build().internal_server_error(err)
    })?;

    tracing::info!(
        command_id = %saved.command_id,
        deployment_hash = %saved.deployment_hash,
        "Command enqueued, agent will poll"
    );

    Ok(JsonResponse::build()
        .set_item(Some(serde_json::json!({
            "command_id": saved.command_id,
            "deployment_hash": saved.deployment_hash,
            "status": saved.status
        })))
        .created("Command enqueued"))
}
