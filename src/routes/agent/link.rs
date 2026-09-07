use crate::connectors::user_service::UserServiceConnector;
use crate::{db, helpers, helpers::AgentPgPool, models};
use actix_web::{post, web, HttpRequest, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct LinkAgentRequest {
    pub session_token: String,
    pub deployment_id: String,
    pub server_fingerprint: serde_json::Value,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LinkAgentResponse {
    pub agent_id: String,
    pub agent_token: String,
    pub deployment_hash: String,
    pub dashboard_url: Option<String>,
}

fn normalized_status_panel_capabilities(capabilities: &[String]) -> serde_json::Value {
    let mut normalized = capabilities.to_vec();
    if !normalized
        .iter()
        .any(|capability| capability == "status_panel")
    {
        normalized.push("status_panel".to_string());
    }
    serde_json::json!(normalized)
}

/// Generate a secure random agent token (86 characters)

/// POST /api/v1/agent/link
///
/// Link an agent to a specific deployment using a session token (OAuth access_token).
/// The session_token proves the user authenticated via /api/v1/agent/login.
/// Stacker validates token ownership, checks the user owns the deployment,
/// then creates or returns an agent with credentials.
#[tracing::instrument(name = "Link agent to deployment", skip_all)]
#[post("/link")]
pub async fn link_handler(
    payload: web::Json<LinkAgentRequest>,
    api_pool: web::Data<sqlx::PgPool>,
    agent_pool: web::Data<AgentPgPool>,
    vault_client: web::Data<helpers::VaultClient>,
    user_service: web::Data<Arc<dyn UserServiceConnector>>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    // 1. Validate session_token by fetching user profile
    let profile = user_service
        .get_user_profile(&payload.session_token)
        .await
        .map_err(|e| {
            tracing::warn!("Invalid session token for link request: {:?}", e);
            helpers::JsonResponse::<LinkAgentResponse>::build()
                .forbidden("Invalid or expired session. Please login again.")
        })?;

    // 2. Verify user owns the requested deployment
    let deployment =
        db::deployment::fetch_by_deployment_hash(api_pool.get_ref(), &payload.deployment_id)
            .await
            .map_err(|e| {
                helpers::JsonResponse::<LinkAgentResponse>::build()
                    .internal_server_error(format!("Database error: {}", e))
            })?;

    let deployment = deployment.ok_or_else(|| {
        helpers::JsonResponse::<LinkAgentResponse>::build().not_found("Deployment not found")
    })?;

    // Check ownership: deployment.user_id stores the queue_key (profile.id),
    // not the email. See UserProfile.id doc comment in types.rs.
    if deployment.user_id.as_deref() != Some(&profile.id) {
        tracing::warn!(
            user = %profile.email,
            user_id = %profile.id,
            deployment_user = ?deployment.user_id,
            deployment_hash = %payload.deployment_id,
            "User attempted to link to deployment they don't own"
        );
        return Err(helpers::JsonResponse::<LinkAgentResponse>::build()
            .forbidden("You do not own this deployment"));
    }

    // 3. Create or reuse agent for this deployment
    let existing_agent =
        db::agent::fetch_by_deployment_hash(agent_pool.as_ref(), &deployment.deployment_hash)
            .await
            .map_err(|e| {
                helpers::JsonResponse::<LinkAgentResponse>::build().internal_server_error(e)
            })?;

    let (agent, agent_token) = if let Some(mut existing) = existing_agent {
        tracing::info!(
            "Agent already exists for deployment {}, reusing",
            deployment.deployment_hash
        );

        // Update system_info with new fingerprint
        existing.system_info = Some(payload.server_fingerprint.clone());
        existing.capabilities = Some(normalized_status_panel_capabilities(&payload.capabilities));
        let existing = db::agent::update(agent_pool.as_ref(), existing)
            .await
            .map_err(|e| {
                helpers::JsonResponse::<LinkAgentResponse>::build().internal_server_error(e)
            })?;

        // Re-issue rather than recovering the old value: verification now
        // compares against the digest in Postgres. The previous shape also
        // regenerated on any Vault error, and stored with a single attempt —
        // no retry, unlike every other site.
        let token = crate::services::agent_token::issue(
            agent_pool.as_ref(),
            vault_client.as_ref(),
            existing.id,
            &deployment.deployment_hash,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to issue agent token on re-link: {}", e);
            helpers::JsonResponse::<LinkAgentResponse>::build().internal_server_error(e)
        })?;

        (existing, token)
    } else {
        // Create new agent
        let mut agent = models::Agent::new(deployment.deployment_hash.clone());
        agent.system_info = Some(payload.server_fingerprint.clone());
        agent.capabilities = Some(normalized_status_panel_capabilities(&payload.capabilities));

        let saved_agent = db::agent::insert(agent_pool.as_ref(), agent)
            .await
            .map_err(|e| {
                helpers::JsonResponse::<LinkAgentResponse>::build().internal_server_error(e)
            })?;

        // Issue the token: digest to Postgres, value to Vault, both awaited,
        // so the caller never receives a credential that cannot be verified.
        let agent_token = crate::services::agent_token::issue(
            agent_pool.as_ref(),
            vault_client.as_ref(),
            saved_agent.id,
            &deployment.deployment_hash,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to issue agent token on link: {}", e);
            helpers::JsonResponse::<LinkAgentResponse>::build().internal_server_error(e)
        })?;

        (saved_agent, agent_token)
    };

    // 4. Audit log
    let audit_log = models::AuditLog::new(
        Some(agent.id),
        Some(deployment.deployment_hash.clone()),
        "agent.linked_via_login".to_string(),
        Some("success".to_string()),
    )
    .with_details(serde_json::json!({
        "user_email": profile.email,
        "deployment_id": deployment.id,
    }))
    .with_ip(
        req.peer_addr()
            .map(|addr| addr.ip().to_string())
            .unwrap_or_default(),
    );

    if let Err(err) = db::agent::log_audit(agent_pool.as_ref(), audit_log).await {
        tracing::warn!("Failed to log agent link audit: {:?}", err);
    }

    tracing::info!(
        agent_id = %agent.id,
        deployment_hash = %deployment.deployment_hash,
        user = %profile.email,
        "Agent linked to deployment via user login"
    );

    Ok(HttpResponse::Ok().json(LinkAgentResponse {
        agent_id: agent.id.to_string(),
        agent_token,
        deployment_hash: deployment.deployment_hash,
        dashboard_url: Some(format!(
            "https://try.direct/dashboard/deployments/{}",
            deployment.id
        )),
    }))
}

#[cfg(test)]
mod tests {
    use super::normalized_status_panel_capabilities;

    #[test]
    fn normalizes_status_panel_capabilities_without_duplicates() {
        let normalized = normalized_status_panel_capabilities(&[
            "docker".to_string(),
            "status_panel".to_string(),
            "npm_credential_source=vault".to_string(),
        ]);

        let capabilities: Vec<String> =
            serde_json::from_value(normalized).expect("capability array");
        assert_eq!(
            capabilities
                .iter()
                .filter(|cap| *cap == "status_panel")
                .count(),
            1
        );
        assert!(capabilities
            .iter()
            .any(|cap| cap == "npm_credential_source=vault"));
    }
}
