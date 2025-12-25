use crate::db;
use crate::helpers::{JsonResponse, VaultClient};
use crate::models::{Command, CommandPriority, User};
use crate::services::agent_dispatcher;
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

#[tracing::instrument(name = "Create command", skip(pg_pool, user, vault_client))]
#[post("")]
pub async fn create_handler(
    user: web::ReqData<Arc<User>>,
    req: web::Json<CreateCommandRequest>,
    pg_pool: web::Data<PgPool>,
    vault_client: web::Data<VaultClient>,
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

    // Optional: push to agent immediately if AGENT_BASE_URL is configured
    if let Ok(agent_base_url) = std::env::var("AGENT_BASE_URL") {
        let payload = serde_json::json!({
            "deployment_hash": saved_command.deployment_hash,
            "command_id": saved_command.command_id,
            "type": saved_command.r#type,
            "priority": format!("{}", priority),
            "parameters": saved_command.parameters,
            "timeout_seconds": saved_command.timeout_seconds,
        });

        match agent_dispatcher::enqueue(
            pg_pool.get_ref(),
            vault_client.get_ref(),
            &saved_command.deployment_hash,
            &agent_base_url,
            &payload,
        )
        .await
        {
            Ok(()) => {
                tracing::info!(
                    "Pushed command {} to agent at {}",
                    saved_command.command_id,
                    agent_base_url
                );
            }
            Err(err) => {
                tracing::warn!(
                    "Agent push failed for command {}: {}",
                    saved_command.command_id,
                    err
                );
            }
        }
    } else {
        tracing::debug!("AGENT_BASE_URL not set; skipping agent push");
    }

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
