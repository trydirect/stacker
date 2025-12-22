use crate::configuration::VaultSettings;
use reqwest::Client;
use serde_json::json;

pub struct VaultClient {
    client: Client,
    address: String,
    token: String,
    agent_path_prefix: String,
}

impl VaultClient {
    pub fn new(settings: &VaultSettings) -> Self {
        Self {
            client: Client::new(),
            address: settings.address.clone(),
            token: settings.token.clone(),
            agent_path_prefix: settings.agent_path_prefix.clone(),
        }
    }

    /// Store agent token in Vault at agent/{deployment_hash}/token
    #[tracing::instrument(name = "Store agent token in Vault", skip(self, token))]
    pub async fn store_agent_token(
        &self,
        deployment_hash: &str,
        token: &str,
    ) -> Result<(), String> {
        let path = format!(
            "{}/v1/{}/{}/token",
            self.address, self.agent_path_prefix, deployment_hash
        );

        let payload = json!({
            "data": {
                "token": token,
                "deployment_hash": deployment_hash
            }
        });

        self.client
            .post(&path)
            .header("X-Vault-Token", &self.token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to store token in Vault: {:?}", e);
                format!("Vault store error: {}", e)
            })?
            .error_for_status()
            .map_err(|e| {
                tracing::error!("Vault returned error status: {:?}", e);
                format!("Vault error: {}", e)
            })?;

        tracing::info!(
            "Stored agent token in Vault for deployment_hash: {}",
            deployment_hash
        );
        Ok(())
    }

    /// Fetch agent token from Vault
    #[tracing::instrument(name = "Fetch agent token from Vault", skip(self))]
    pub async fn fetch_agent_token(&self, deployment_hash: &str) -> Result<String, String> {
        let path = format!(
            "{}/v1/{}/{}/token",
            self.address, self.agent_path_prefix, deployment_hash
        );

        let response = self
            .client
            .get(&path)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch token from Vault: {:?}", e);
                format!("Vault fetch error: {}", e)
            })?;

        if response.status() == 404 {
            return Err("Token not found in Vault".to_string());
        }

        let vault_response: serde_json::Value = response
            .error_for_status()
            .map_err(|e| {
                tracing::error!("Vault returned error status: {:?}", e);
                format!("Vault error: {}", e)
            })?
            .json()
            .await
            .map_err(|e| {
                tracing::error!("Failed to parse Vault response: {:?}", e);
                format!("Vault parse error: {}", e)
            })?;

        vault_response["data"]["data"]["token"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| {
                tracing::error!("Token not found in Vault response");
                "Token not in Vault response".to_string()
            })
    }

    /// Delete agent token from Vault
    #[tracing::instrument(name = "Delete agent token from Vault", skip(self))]
    pub async fn delete_agent_token(&self, deployment_hash: &str) -> Result<(), String> {
        let path = format!(
            "{}/v1/{}/{}/token",
            self.address, self.agent_path_prefix, deployment_hash
        );

        self.client
            .delete(&path)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to delete token from Vault: {:?}", e);
                format!("Vault delete error: {}", e)
            })?
            .error_for_status()
            .map_err(|e| {
                tracing::error!("Vault returned error status: {:?}", e);
                format!("Vault error: {}", e)
            })?;

        tracing::info!(
            "Deleted agent token from Vault for deployment_hash: {}",
            deployment_hash
        );
        Ok(())
    }
}
