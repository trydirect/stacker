use crate::{db, helpers, models};
use actix_web::{post, web, HttpRequest, Responder, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct RegisterAgentRequest {
    pub deployment_hash: String,
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

/// Generate a secure random agent token (86 characters)
fn generate_agent_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut rng = rand::thread_rng();
    (0..86)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[tracing::instrument(name = "Register agent", skip(pg_pool, vault_client, req))]
#[post("/register")]
pub async fn register_handler(
    user: web::ReqData<Arc<models::User>>,
    payload: web::Json<RegisterAgentRequest>,
    pg_pool: web::Data<PgPool>,
    vault_client: web::Data<helpers::VaultClient>,
    req: HttpRequest,
) -> Result<impl Responder> {
    // Check if agent already exists for this deployment
    let existing_agent = db::agent::fetch_by_deployment_hash(pg_pool.get_ref(), &payload.deployment_hash)
        .await
        .map_err(|err| helpers::JsonResponse::<RegisterAgentResponse>::build().internal_server_error(err))?;

    if existing_agent.is_some() {
        return Err(helpers::JsonResponse::<RegisterAgentResponse>::build()
            .bad_request("Agent already registered for this deployment".to_string()));
    }

    // Create new agent
    let mut agent = models::Agent::new(payload.deployment_hash.clone());
    agent.capabilities = Some(serde_json::json!(payload.capabilities));
    agent.version = Some(payload.agent_version.clone());
    agent.system_info = Some(payload.system_info.clone());

    // Generate agent token
    let agent_token = generate_agent_token();

    // Store token in Vault
    vault_client
        .store_agent_token(&payload.deployment_hash, &agent_token)
        .await
        .map_err(|err| {
            tracing::error!("Failed to store token in Vault: {:?}", err);
            helpers::JsonResponse::<RegisterAgentResponse>::build()
                .internal_server_error(format!("Failed to store token: {}", err))
        })?;

    // Save agent to database
    let saved_agent = db::agent::insert(pg_pool.get_ref(), agent)
        .await
        .map_err(|err| {
            tracing::error!("Failed to save agent: {:?}", err);
            // Clean up Vault token if DB insert fails
            let vault = vault_client.clone();
            let hash = payload.deployment_hash.clone();
            actix_web::rt::spawn(async move {
                let _ = vault.delete_agent_token(&hash).await;
            });
            helpers::JsonResponse::<RegisterAgentResponse>::build().internal_server_error(err)
        })?;

    // Log registration in audit log
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
    .with_ip(req.peer_addr().map(|addr| addr.ip().to_string()).unwrap_or_default());

    let _ = db::agent::log_audit(pg_pool.get_ref(), audit_log).await;

    let response = RegisterAgentResponse {
        agent_id: saved_agent.id.to_string(),
        agent_token,
        dashboard_version: "2.0.0".to_string(),
        supported_api_versions: vec!["1.0".to_string()],
    };

    tracing::info!(
        "Agent registered: {} for deployment: {}",
        saved_agent.id,
        payload.deployment_hash
    );

    Ok(helpers::JsonResponse::build().set_item(Some(response)).ok("Agent registered"))
}
