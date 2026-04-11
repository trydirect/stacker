use crate::db;
use crate::forms::status_panel;
use crate::helpers::{AgentPgPool, JsonResponse};
use crate::models::{Command, CommandPriority, User};
use actix_web::{post, web, Responder, Result};
use serde::Deserialize;
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

#[tracing::instrument(name = "Agent enqueue command", skip_all)]
#[post("/commands/enqueue")]
pub async fn enqueue_handler(
    user: web::ReqData<Arc<User>>,
    payload: web::Json<EnqueueRequest>,
    agent_pool: web::Data<AgentPgPool>,
) -> Result<impl Responder> {
    if payload.deployment_hash.trim().is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("deployment_hash is required"));
    }

    if payload.command_type.trim().is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("command_type is required"));
    }

    // Verify deployment belongs to the requesting user
    let deployment =
        db::deployment::fetch_by_deployment_hash(agent_pool.as_ref(), &payload.deployment_hash)
            .await
            .map_err(|err| JsonResponse::<()>::build().internal_server_error(err))?;

    match &deployment {
        Some(d) if d.user_id.as_deref() == Some(&user.id) => {}
        _ => {
            return Err(JsonResponse::<()>::build().not_found("Deployment not found"));
        }
    }

    // Validate parameters
    let validated_parameters =
        status_panel::validate_command_parameters(&payload.command_type, &payload.parameters)
            .map_err(|err| JsonResponse::<()>::build().bad_request(err))?;

    // If runtime=kata requested, verify agent supports it
    if let Some(ref params) = validated_parameters {
        if params.get("runtime").and_then(|v| v.as_str()) == Some("kata") {
            let agent =
                db::agent::fetch_by_deployment_hash(agent_pool.as_ref(), &payload.deployment_hash)
                    .await
                    .map_err(|err| {
                        tracing::error!("Failed to fetch agent: {}", err);
                        JsonResponse::<()>::build().internal_server_error(err)
                    })?;

            let has_kata = agent
                .as_ref()
                .and_then(|a| a.capabilities.as_ref())
                .and_then(|c| serde_json::from_value::<Vec<String>>(c.clone()).ok())
                .map(|caps| caps.iter().any(|c| c == "kata"))
                .unwrap_or(false);

            if !has_kata {
                return Err(JsonResponse::<()>::build().bad_request(
                    "Agent does not support Kata runtime. Check agent capabilities at GET /deployments/{hash}/capabilities"
                ));
            }
        }
    }

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
    let saved = db::command::insert(agent_pool.as_ref(), &command)
        .await
        .map_err(|err| {
            tracing::error!("Failed to insert command: {}", err);
            JsonResponse::<()>::build().internal_server_error(err)
        })?;

    // Add to queue - agent will poll and pick it up
    db::command::add_to_queue(
        agent_pool.as_ref(),
        &saved.command_id,
        &saved.deployment_hash,
        &priority,
    )
    .await
    .map_err(|err| {
        tracing::error!("Failed to add command to queue: {}", err);
        JsonResponse::<()>::build().internal_server_error(err)
    })?;

    // Extract runtime for tracing
    let runtime = validated_parameters
        .as_ref()
        .and_then(|p| p.get("runtime"))
        .and_then(|v| v.as_str())
        .unwrap_or("runc");

    tracing::info!(
        command_id = %saved.command_id,
        deployment_hash = %saved.deployment_hash,
        command_type = %payload.command_type,
        runtime = %runtime,
        "Command enqueued, agent will poll"
    );

    Ok(JsonResponse::build()
        .set_item(Some(saved))
        .created("Command enqueued"))
}
