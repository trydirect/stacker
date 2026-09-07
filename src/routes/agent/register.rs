use crate::{db, helpers, helpers::AgentPgPool, models};
use actix_web::{post, web, HttpRequest, HttpResponse, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct RegisterAgentRequest {
    pub deployment_hash: String,
    #[allow(dead_code)]
    pub public_key: Option<String>,
    pub capabilities: Vec<String>,
    pub system_info: serde_json::Value,
    pub agent_version: String,
}

#[derive(Debug, Serialize, Default)]
pub struct RegisterAgentResponse {
    pub agent_id: String,
    pub agent_token: String,
    pub dashboard_version: String,
    pub supported_api_versions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RegisterAgentResponseWrapper {
    pub data: RegisterAgentResponseData,
}

#[derive(Debug, Serialize)]
pub struct RegisterAgentResponseData {
    pub item: RegisterAgentResponse,
}

/// Generate a secure random agent token (86 characters)

#[tracing::instrument(name = "Register agent", skip_all)]
#[post("/register")]
pub async fn register_handler(
    payload: web::Json<RegisterAgentRequest>,
    agent_pool: web::Data<AgentPgPool>,
    vault_client: web::Data<helpers::VaultClient>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    // 1. Check if agent already registered (idempotent operation)
    let existing_agent =
        db::agent::fetch_by_deployment_hash(agent_pool.as_ref(), &payload.deployment_hash)
            .await
            .map_err(|err| {
                helpers::JsonResponse::<RegisterAgentResponse>::build().internal_server_error(err)
            })?;

    if let Some(mut existing) = existing_agent {
        tracing::info!(
            "Agent already registered for deployment {}, returning existing",
            payload.deployment_hash
        );

        // Refresh agent metadata for existing registrations
        existing.capabilities = Some(serde_json::json!(payload.capabilities));
        existing.version = Some(payload.agent_version.clone());
        existing.system_info = Some(payload.system_info.clone());
        let existing = db::agent::update(agent_pool.as_ref(), existing)
            .await
            .map_err(|err| {
                tracing::error!("Failed to update agent metadata: {:?}", err);
                helpers::JsonResponse::<RegisterAgentResponse>::build().internal_server_error(err)
            })?;

        // Re-issue rather than reading the old token back. Verification now
        // compares against the digest in Postgres, so the previous
        // `fetch_agent_token(..).unwrap_or_else(regenerate)` has no value to
        // recover — and it regenerated on *any* Vault error, rotating a
        // healthy agent's credential over a network blip.
        let agent_token = crate::services::agent_token::issue(
            agent_pool.as_ref(),
            vault_client.as_ref(),
            existing.id,
            &payload.deployment_hash,
        )
        .await
        .map_err(|err| {
            tracing::error!("Failed to issue agent token on re-registration: {}", err);
            helpers::JsonResponse::<RegisterAgentResponse>::build().internal_server_error(err)
        })?;

        let response = RegisterAgentResponseWrapper {
            data: RegisterAgentResponseData {
                item: RegisterAgentResponse {
                    agent_id: existing.id.to_string(),
                    agent_token,
                    dashboard_version: "2.0.0".to_string(),
                    supported_api_versions: vec!["1.0".to_string()],
                },
            },
        };

        return Ok(HttpResponse::Ok().json(response));
    }

    // 3. Create new agent
    let mut agent = models::Agent::new(payload.deployment_hash.clone());
    agent.capabilities = Some(serde_json::json!(payload.capabilities));
    agent.version = Some(payload.agent_version.clone());
    agent.system_info = Some(payload.system_info.clone());

    // 4. Insert to DB first (source of truth)
    let saved_agent = db::agent::insert(agent_pool.as_ref(), agent)
        .await
        .map_err(|err| {
            tracing::error!("Failed to save agent to DB: {:?}", err);
            helpers::JsonResponse::<RegisterAgentResponse>::build().internal_server_error(err)
        })?;

    // 5. Issue the token: digest to Postgres, value to Vault, both awaited.
    //
    // Previously this was a detached spawn and the response went out
    // immediately, so a failed store handed the agent a credential nothing
    // could verify — and a poll arriving first was rejected with "Token not
    // found in Vault", which is what made tests/agent_command_flow.rs flaky.
    let agent_token = crate::services::agent_token::issue(
        agent_pool.as_ref(),
        vault_client.as_ref(),
        saved_agent.id,
        &payload.deployment_hash,
    )
    .await
    .map_err(|err| {
        tracing::error!("Failed to issue agent token on registration: {}", err);
        helpers::JsonResponse::<RegisterAgentResponse>::build().internal_server_error(err)
    })?;

    let audit_log = models::AuditLog::new(
        Some(saved_agent.id),
        Some(payload.deployment_hash.clone()),
        "agent.registered".to_string(),
        Some("success".to_string()),
    )
    .with_details(serde_json::json!({
        "version": payload.agent_version,
        "capabilities": payload.capabilities,
    }))
    .with_ip(
        req.peer_addr()
            .map(|addr| addr.ip().to_string())
            .unwrap_or_default(),
    );

    if let Err(err) = db::agent::log_audit(agent_pool.as_ref(), audit_log).await {
        tracing::warn!("Failed to log agent registration audit: {:?}", err);
    }

    let response = RegisterAgentResponseWrapper {
        data: RegisterAgentResponseData {
            item: RegisterAgentResponse {
                agent_id: saved_agent.id.to_string(),
                agent_token,
                dashboard_version: "2.0.0".to_string(),
                supported_api_versions: vec!["1.0".to_string()],
            },
        },
    };

    tracing::info!(
        "Agent registered: {} for deployment: {}",
        saved_agent.id,
        payload.deployment_hash
    );

    Ok(HttpResponse::Created().json(response))
}
