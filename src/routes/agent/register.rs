use crate::{db, helpers, models};
use actix_web::{post, web, HttpRequest, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

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

#[derive(Debug, Serialize)]
pub struct RegisterAgentResponseWrapper {
    pub data: RegisterAgentResponseData,
}

#[derive(Debug, Serialize)]
pub struct RegisterAgentResponseData {
    pub item: RegisterAgentResponse,
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
    payload: web::Json<RegisterAgentRequest>,
    pg_pool: web::Data<PgPool>,
    vault_client: web::Data<helpers::VaultClient>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let existing_agent =
        db::agent::fetch_by_deployment_hash(pg_pool.get_ref(), &payload.deployment_hash)
            .await
            .map_err(|err| {
                helpers::JsonResponse::<RegisterAgentResponse>::build().internal_server_error(err)
            })?;

    if existing_agent.is_some() {
        return Ok(HttpResponse::Conflict().json(serde_json::json!({
            "message": "Agent already registered for this deployment",
            "status_code": 409
        })));
    }

    let mut agent = models::Agent::new(payload.deployment_hash.clone());
    agent.capabilities = Some(serde_json::json!(payload.capabilities));
    agent.version = Some(payload.agent_version.clone());
    agent.system_info = Some(payload.system_info.clone());

    let agent_token = generate_agent_token();

    if let Err(err) = vault_client
        .store_agent_token(&payload.deployment_hash, &agent_token)
        .await
    {
        tracing::warn!(
            "Failed to store token in Vault (continuing anyway): {:?}",
            err
        );
    }

    let saved_agent = db::agent::insert(pg_pool.get_ref(), agent)
        .await
        .map_err(|err| {
            tracing::error!("Failed to save agent: {:?}", err);
            let vault = vault_client.clone();
            let hash = payload.deployment_hash.clone();
            actix_web::rt::spawn(async move {
                let _ = vault.delete_agent_token(&hash).await;
            });
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

    if let Err(err) = db::agent::log_audit(pg_pool.get_ref(), audit_log).await {
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
