use crate::{db, helpers};
use helpers::{AgentClient, VaultClient};
use serde_json::Value;
use sqlx::PgPool;

async fn ensure_agent_credentials(
    pg: &PgPool,
    vault: &VaultClient,
    deployment_hash: &str,
) -> Result<(String, String), String> {
    let agent = db::agent::fetch_by_deployment_hash(pg, deployment_hash)
        .await
        .map_err(|e| format!("DB error: {}", e))?
        .ok_or_else(|| "Agent not found for deployment_hash".to_string())?;

    let token = vault
        .fetch_agent_token(&agent.deployment_hash)
        .await
        .map_err(|e| format!("Vault error: {}", e))?;

    Ok((agent.id.to_string(), token))
}

async fn handle_resp(resp: reqwest::Response) -> Result<(), String> {
    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Err(format!("Agent request failed: {} - {}", status, text))
}

#[tracing::instrument(name = "AgentDispatcher enqueue", skip(pg, vault, command), fields(deployment_hash = %deployment_hash, agent_base_url = %agent_base_url))]
pub async fn enqueue(
    pg: &PgPool,
    vault: &VaultClient,
    deployment_hash: &str,
    agent_base_url: &str,
    command: &Value,
) -> Result<(), String> {
    let (agent_id, agent_token) = ensure_agent_credentials(pg, vault, deployment_hash).await?;
    let client = AgentClient::new(agent_base_url, agent_id, agent_token);
    tracing::info!(deployment_hash = %deployment_hash, "Dispatching enqueue to agent");
    let resp = client
        .commands_enqueue(command)
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;
    handle_resp(resp).await
}

#[tracing::instrument(name = "AgentDispatcher execute", skip(pg, vault, command), fields(deployment_hash = %deployment_hash, agent_base_url = %agent_base_url))]
pub async fn execute(
    pg: &PgPool,
    vault: &VaultClient,
    deployment_hash: &str,
    agent_base_url: &str,
    command: &Value,
) -> Result<(), String> {
    let (agent_id, agent_token) = ensure_agent_credentials(pg, vault, deployment_hash).await?;
    let client = AgentClient::new(agent_base_url, agent_id, agent_token);
    tracing::info!(deployment_hash = %deployment_hash, "Dispatching execute to agent");
    let resp = client
        .commands_execute(command)
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;
    handle_resp(resp).await
}

#[tracing::instrument(name = "AgentDispatcher report", skip(pg, vault, result), fields(deployment_hash = %deployment_hash, agent_base_url = %agent_base_url))]
pub async fn report(
    pg: &PgPool,
    vault: &VaultClient,
    deployment_hash: &str,
    agent_base_url: &str,
    result: &Value,
) -> Result<(), String> {
    let (agent_id, agent_token) = ensure_agent_credentials(pg, vault, deployment_hash).await?;
    let client = AgentClient::new(agent_base_url, agent_id, agent_token);
    tracing::info!(deployment_hash = %deployment_hash, "Dispatching report to agent");
    let resp = client
        .commands_report(result)
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;
    handle_resp(resp).await
}

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

#[tracing::instrument(name = "AgentDispatcher wait", skip(pg, vault), fields(deployment_hash = %deployment_hash, agent_base_url = %agent_base_url))]
pub async fn wait(
    pg: &PgPool,
    vault: &VaultClient,
    deployment_hash: &str,
    agent_base_url: &str,
) -> Result<reqwest::Response, String> {
    let (agent_id, agent_token) = ensure_agent_credentials(pg, vault, deployment_hash).await?;
    let client = AgentClient::new(agent_base_url, agent_id, agent_token);
    tracing::info!(deployment_hash = %deployment_hash, "Agent long-poll wait");
    client.wait(deployment_hash).await.map_err(|e| format!("HTTP error: {}", e))
}
