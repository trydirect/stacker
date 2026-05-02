use crate::configuration::Settings;
use crate::db;
use crate::forms::status_panel;
use crate::helpers::{
    extract_capabilities, has_capability, has_capability_value, AgentPgPool, JsonResponse,
    NPM_CREDENTIAL_SOURCE_KEY,
};
use crate::models::{Command, CommandPriority, User};
use crate::routes::legacy_installations::resolve_owned_deployment_by_hash;
use actix_web::{post, web, Responder, Result};
use serde::Deserialize;
use std::sync::Arc;

const CONFIGURE_PROXY_CAPABILITY_MODE_ENV: &str = "STACKER_CONFIGURE_PROXY_CAPABILITY_MODE";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigureProxyCapabilityMode {
    Warn,
    Enforce,
}

impl ConfigureProxyCapabilityMode {
    fn from_env() -> Self {
        Self::from_value(
            std::env::var(CONFIGURE_PROXY_CAPABILITY_MODE_ENV)
                .ok()
                .as_deref(),
        )
    }

    fn from_value(value: Option<&str>) -> Self {
        match value.unwrap_or("warn").trim().to_ascii_lowercase().as_str() {
            "enforce" | "true" | "1" => Self::Enforce,
            _ => Self::Warn,
        }
    }
}

fn configure_proxy_requires_vault_capability(capabilities: &[String]) -> bool {
    has_capability_value(capabilities, NPM_CREDENTIAL_SOURCE_KEY, "vault")
}

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
    settings: web::Data<Settings>,
) -> Result<impl Responder> {
    if payload.deployment_hash.trim().is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("deployment_hash is required"));
    }

    if payload.command_type.trim().is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("command_type is required"));
    }

    resolve_owned_deployment_by_hash(
        agent_pool.as_ref(),
        settings.get_ref(),
        user.as_ref(),
        &payload.deployment_hash,
    )
    .await?;

    // Validate parameters
    let validated_parameters =
        status_panel::validate_command_parameters(&payload.command_type, &payload.parameters)
            .map_err(|err| JsonResponse::<()>::build().bad_request(err))?;

    let agent = if payload.command_type == "configure_proxy"
        || validated_parameters
            .as_ref()
            .and_then(|params| params.get("runtime"))
            .and_then(|value| value.as_str())
            == Some("kata")
    {
        db::agent::fetch_by_deployment_hash(agent_pool.as_ref(), &payload.deployment_hash)
            .await
            .map_err(|err| {
                tracing::error!("Failed to fetch agent: {}", err);
                JsonResponse::<()>::build().internal_server_error(err)
            })?
    } else {
        None
    };

    // If runtime=kata requested, verify agent supports it
    if let Some(ref params) = validated_parameters {
        if params.get("runtime").and_then(|v| v.as_str()) == Some("kata") {
            let has_kata = agent
                .as_ref()
                .map(|agent| extract_capabilities(agent.capabilities.clone()))
                .map(|capabilities| has_capability(&capabilities, "kata"))
                .unwrap_or(false);

            if !has_kata {
                return Err(JsonResponse::<()>::build().bad_request(
                    "Agent does not support Kata runtime. Check agent capabilities at GET /deployments/{hash}/capabilities"
                ));
            }
        }
    }

    if payload.command_type == "configure_proxy" {
        let capabilities = agent
            .as_ref()
            .map(|agent| extract_capabilities(agent.capabilities.clone()))
            .unwrap_or_default();

        if !configure_proxy_requires_vault_capability(&capabilities) {
            let message = "Agent does not advertise npm_credential_source=vault. Re-link the Status Panel agent or update the installer before running configure_proxy.";
            match ConfigureProxyCapabilityMode::from_env() {
                ConfigureProxyCapabilityMode::Warn => {
                    tracing::warn!(
                        deployment_hash = %payload.deployment_hash,
                        capabilities = ?capabilities,
                        "configure_proxy queued without Vault capability: {}",
                        message
                    );
                }
                ConfigureProxyCapabilityMode::Enforce => {
                    return Err(JsonResponse::<()>::build().bad_request(message));
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_proxy_capability_mode_defaults_to_warn() {
        assert_eq!(
            ConfigureProxyCapabilityMode::from_value(None),
            ConfigureProxyCapabilityMode::Warn
        );
    }

    #[test]
    fn configure_proxy_capability_mode_accepts_enforce_flag() {
        assert_eq!(
            ConfigureProxyCapabilityMode::from_value(Some("enforce")),
            ConfigureProxyCapabilityMode::Enforce
        );
    }

    #[test]
    fn configure_proxy_requires_vault_capability_marker() {
        assert!(configure_proxy_requires_vault_capability(&[
            "npm_credential_source=vault".to_string()
        ]));
        assert!(!configure_proxy_requires_vault_capability(&[
            "status_panel".to_string()
        ]));
    }
}
