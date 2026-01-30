use crate::db;
use crate::forms::status_panel;
use crate::helpers::JsonResponse;
use crate::models::{Command, CommandPriority, User};
use crate::services::VaultService;
use crate::configuration::Settings;
use actix_web::{post, web, Responder, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
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

#[tracing::instrument(name = "Create command", skip(pg_pool, user, settings))]
#[post("")]
pub async fn create_handler(
    user: web::ReqData<Arc<User>>,
    req: web::Json<CreateCommandRequest>,
    pg_pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
) -> Result<impl Responder> {
    if req.deployment_hash.trim().is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("deployment_hash is required"));
    }

    if req.command_type.trim().is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("command_type is required"));
    }

    let validated_parameters =
        status_panel::validate_command_parameters(&req.command_type, &req.parameters).map_err(
            |err| {
                tracing::warn!("Invalid command payload: {}", err);
                JsonResponse::<()>::build().bad_request(err)
            },
        )?;

    // For deploy_app commands, enrich with compose_content from Vault if not provided
    let final_parameters = if req.command_type == "deploy_app" {
        enrich_deploy_app_with_compose(&req.deployment_hash, validated_parameters, &settings.vault).await
    } else {
        validated_parameters
    };

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

    if let Some(params) = &final_parameters {
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

    // Add to queue - agent will poll and pick it up
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
        command_id = %saved_command.command_id,
        deployment_hash = %saved_command.deployment_hash,
        "Command created and queued, agent will poll"
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

/// Enrich deploy_app command parameters with compose_content from Vault
/// If compose_content is already provided in the request, keep it as-is
async fn enrich_deploy_app_with_compose(
    deployment_hash: &str,
    params: Option<serde_json::Value>,
    vault_settings: &crate::configuration::VaultSettings,
) -> Option<serde_json::Value> {
    let mut params = params.unwrap_or_else(|| json!({}));

    // If compose_content is already provided, use it as-is
    if params.get("compose_content").and_then(|v| v.as_str()).is_some() {
        tracing::debug!("deploy_app already has compose_content, skipping Vault fetch");
        return Some(params);
    }

    // Try to fetch compose content from Vault using settings
    let vault = match VaultService::from_settings(vault_settings) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Failed to initialize Vault: {}, cannot enrich deploy_app", e);
            return Some(params);
        }
    };

    // Fetch compose config (stored under "_compose" key)
    match vault.fetch_app_config(deployment_hash, "_compose").await {
        Ok(compose_config) => {
            tracing::info!(
                deployment_hash = %deployment_hash,
                "Enriched deploy_app command with compose_content from Vault"
            );
            if let Some(obj) = params.as_object_mut() {
                obj.insert("compose_content".to_string(), json!(compose_config.content));
            }
        }
        Err(e) => {
            tracing::warn!(
                deployment_hash = %deployment_hash,
                error = %e,
                "Failed to fetch compose from Vault, deploy_app may fail if compose not on disk"
            );
        }
    }

    Some(params)
}
