use crate::{db, helpers};
use helpers::VaultClient;
use sqlx::PgPool;

/// Rotate token by writing the new value into Vault.
/// Agent is expected to pull the latest token from Vault.
#[tracing::instrument(name = "AgentDispatcher rotate_token", skip(pg, vault, new_token), fields(deployment_hash = %deployment_hash))]
pub async fn rotate_token(
    pg: &PgPool,
    vault: &VaultClient,
    deployment_hash: &str,
    new_token: &str,
) -> Result<(), String> {
    // Ensure agent exists for the deployment
    let _ = db::agent::fetch_by_deployment_hash(pg, deployment_hash)
        .await
        .map_err(|e| format!("DB error: {}", e))?
        .ok_or_else(|| "Agent not found for deployment_hash".to_string())?;

    tracing::info!(deployment_hash = %deployment_hash, "Storing rotated token in Vault");
    vault
        .store_agent_token(deployment_hash, new_token)
        .await
        .map_err(|e| format!("Vault store error: {}", e))?;

    Ok(())
}
