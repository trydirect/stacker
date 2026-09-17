//! Reissuing an agent's bearer token.
//!
//! An operator action, so it lives behind the API and is authorised as the
//! deployment's owner. It used to exist only in the `console` binary, which
//! talks to Postgres and Vault directly and is built only with the `explain`
//! feature — so on a release image the one command that restores a locked-out
//! agent was unavailable.

use crate::db;
use crate::helpers::{AgentPgPool, JsonResponse, VaultClient};
use crate::models;
use actix_web::{post, web, Responder, Result};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct RotateTokenResponse {
    pub deployment_hash: String,
    pub token_hash_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// `POST /api/v1/agent/rotate-token/{deployment_hash}`
///
/// Mints a new token, records its digest and publishes the value to Vault. The
/// agent adopts it from Vault on its next refresh — about a minute — so no
/// reinstall is needed.
///
/// **The token is not returned.** The agent's only source is Vault, so nothing
/// here needs to see it, and printing a live credential into a terminal, a
/// shell history and a CI log is worth avoiding. The console command returned
/// it because it had no other way to deliver it.
#[tracing::instrument(name = "Rotate agent token", skip_all)]
#[post("/rotate-token/{deployment_hash}")]
pub async fn rotate_token_handler(
    path: web::Path<String>,
    pg_pool: web::Data<PgPool>,
    agent_pool: web::Data<AgentPgPool>,
    vault_client: web::Data<VaultClient>,
    settings: web::Data<crate::configuration::Settings>,
    caller_agent: Option<web::ReqData<Arc<models::Agent>>>,
    caller_user: Option<web::ReqData<Arc<models::User>>>,
) -> Result<impl Responder> {
    let deployment_hash = path.into_inner();

    crate::routes::agent::guard::authorize_deployment_access(
        pg_pool.get_ref(),
        settings.get_ref(),
        &deployment_hash,
        caller_agent.as_deref(),
        caller_user.as_deref(),
    )
    .await?;

    let agent = db::agent::fetch_by_deployment_hash(agent_pool.as_ref(), &deployment_hash)
        .await
        .map_err(|err| JsonResponse::<RotateTokenResponse>::build().internal_server_error(err))?
        .ok_or_else(|| {
            // Same reasoning as the guard: an agent that does not exist and one
            // belonging to someone else must be indistinguishable.
            JsonResponse::<String>::not_found("Agent not found for this deployment")
        })?;

    crate::services::agent_token::issue(
        agent_pool.as_ref(),
        vault_client.as_ref(),
        agent.id,
        &deployment_hash,
    )
    .await
    .map_err(|err| {
        tracing::error!("Failed to rotate agent token: {}", err);
        JsonResponse::<RotateTokenResponse>::build().internal_server_error(err)
    })?;

    let refreshed = db::agent::fetch_by_id(agent_pool.as_ref(), agent.id)
        .await
        .ok()
        .flatten();

    Ok(JsonResponse::build()
        .set_item(RotateTokenResponse {
            deployment_hash,
            token_hash_updated_at: refreshed.and_then(|a| a.token_hash_updated_at),
        })
        .ok("Agent token rotated; the agent adopts it from Vault within about a minute"))
}
