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

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{web, App, HttpResponse, HttpServer};
    use serde_json::Value;
    use std::net::TcpListener;

    async fn mock_store(body: web::Json<Value>) -> HttpResponse {
        // Expect { data: { token, deployment_hash } }
        if body["data"]["token"].is_string() && body["data"]["deployment_hash"].is_string() {
            HttpResponse::NoContent().finish()
        } else {
            HttpResponse::BadRequest().finish()
        }
    }

    async fn mock_fetch(path: web::Path<(String, String)>) -> HttpResponse {
        let (_prefix, deployment_hash) = path.into_inner();
        let resp = json!({
            "data": {
                "data": {
                    "token": "test-token-123",
                    "deployment_hash": deployment_hash
                }
            }
        });
        HttpResponse::Ok().json(resp)
    }

    async fn mock_delete() -> HttpResponse {
        HttpResponse::NoContent().finish()
    }

    #[tokio::test]
    async fn test_vault_client_store_fetch_delete() {
        // Start mock Vault server
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind port");
        let port = listener.local_addr().unwrap().port();
        let address = format!("http://127.0.0.1:{}", port);
        let prefix = "agent".to_string();

        let server = HttpServer::new(|| {
            App::new()
                // POST /v1/{prefix}/{deployment_hash}/token
                .route("/v1/{prefix}/{deployment_hash}/token", web::post().to(mock_store))
                // GET /v1/{prefix}/{deployment_hash}/token
                .route("/v1/{prefix}/{deployment_hash}/token", web::get().to(mock_fetch))
                // DELETE /v1/{prefix}/{deployment_hash}/token
                .route("/v1/{prefix}/{deployment_hash}/token", web::delete().to(mock_delete))
        })
        .listen(listener)
        .unwrap()
        .run();

        let _ = tokio::spawn(server);

        // Configure client
        let settings = VaultSettings {
            address: address.clone(),
            token: "dev-token".to_string(),
            agent_path_prefix: prefix.clone(),
        };
        let client = VaultClient::new(&settings);
        let dh = "dep_test_abc";

        // Store
        client
            .store_agent_token(dh, "test-token-123")
            .await
            .expect("store token");

        // Fetch
        let fetched = client.fetch_agent_token(dh).await.expect("fetch token");
        assert_eq!(fetched, "test-token-123");

        // Delete
        client.delete_agent_token(dh).await.expect("delete token");
    }
}
