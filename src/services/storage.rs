//! Backup-storage connector (WS-1 1.3): management operations against the
//! storage backend — device registration and file deletion (retention). The
//! heavy data path (upload/download) is the agent's job; this client handles
//! the control-plane calls Stacker makes.
//!
//! Auth is OAuth2 `client_credentials` (service account); the token is cached
//! and refreshed on 401. Transient failures (5xx / connect / timeout) are
//! retried with a short backoff.

use serde_json::Value;
use std::time::Duration;

/// Retry attempts for a transient request.
const RETRIES: usize = 3;

#[derive(Debug, Clone)]
pub struct StorageClient {
    http: reqwest::Client,
    /// Storage base endpoint (upload/download/control plane).
    endpoint: String,
    /// OAuth2 token endpoint (not a secret).
    token_url: String,
    client_id: String,
    client_secret: String,
    /// Cached bearer token.
    token: Option<String>,
}

impl StorageClient {
    pub fn new(
        endpoint: impl Into<String>,
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            token_url: token_url.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            token: None,
        }
    }

    /// Obtain (and cache) a bearer token via OAuth2 `client_credentials`.
    async fn bearer(&mut self) -> Result<String, String> {
        if let Some(token) = &self.token {
            return Ok(token.clone());
        }
        let resp = self
            .http
            .post(&self.token_url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
            ])
            .send()
            .await
            .map_err(|e| format!("token request failed: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("token request rejected: {}", resp.status()));
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("token response invalid: {}", e))?;
        let token = body
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or("token response missing access_token")?
            .to_string();
        self.token = Some(token.clone());
        Ok(token)
    }

    /// Send a request with bearer auth and retry. Retries on 5xx / connect /
    /// timeout; refreshes the token and retries once on 401.
    async fn send(
        &mut self,
        method: reqwest::Method,
        url: &str,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let token = self.bearer().await?;
            let mut req = self.http.request(method.clone(), url).bearer_auth(&token);
            if let Some(payload) = body {
                req = req.json(payload);
            }
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let text = resp.text().await.unwrap_or_default();
                        return Ok(serde_json::from_str(&text).unwrap_or(Value::Null));
                    }
                    // Expired/invalid token: drop the cache and retry.
                    if status.as_u16() == 401 {
                        self.token = None;
                        if attempt < RETRIES {
                            continue;
                        }
                    }
                    if status.is_server_error() && attempt < RETRIES {
                        tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
                        continue;
                    }
                    return Err(format!("storage request rejected: {}", status));
                }
                Err(error) => {
                    if attempt < RETRIES {
                        tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
                        continue;
                    }
                    return Err(format!("storage request failed: {}", error));
                }
            }
        }
    }

    /// Register a device (the agent's `device_id`) with the storage backend.
    pub async fn register_device(&mut self, device_id: &str) -> Result<Value, String> {
        let url = format!("{}/connect", self.endpoint);
        let body = serde_json::json!({ "device_id": device_id });
        self.send(reqwest::Method::POST, &url, Some(&body)).await
    }

    /// Delete a stored archive (retention cleanup for a pruned run).
    pub async fn delete_file(&mut self, remote_file_id: &str) -> Result<(), String> {
        let url = format!("{}/upload/files/{}", self.endpoint, remote_file_id);
        self.send(reqwest::Method::DELETE, &url, None)
            .await
            .map(|_| ())
    }

    /// List stored archives (restore discovery).
    pub async fn list_files(&mut self) -> Result<Value, String> {
        let url = format!("{}/upload/files", self.endpoint);
        self.send(reqwest::Method::GET, &url, None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn token_mock(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "access_token": "tok-123", "token_type": "Bearer" }),
            ))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn test_delete_file_authenticates_and_calls_delete() {
        let server = MockServer::start().await;
        token_mock(&server).await;
        Mock::given(method("DELETE"))
            .and(path("/upload/files/f-1"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let mut client = StorageClient::new(
            server.uri(),
            format!("{}/oauth/token", server.uri()),
            "cid",
            "csecret",
        );
        assert!(client.delete_file("f-1").await.is_ok());
    }

    #[tokio::test]
    async fn test_register_device_posts_connect() {
        let server = MockServer::start().await;
        token_mock(&server).await;
        Mock::given(method("POST"))
            .and(path("/connect"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "device_id": "agent-1", "account_id": 2 })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let mut client = StorageClient::new(
            server.uri(),
            format!("{}/oauth/token", server.uri()),
            "cid",
            "csecret",
        );
        let result = client.register_device("agent-1").await.unwrap();
        assert_eq!(result["account_id"], 2);
    }

    #[tokio::test]
    async fn test_transient_5xx_is_retried_then_succeeds() {
        let server = MockServer::start().await;
        token_mock(&server).await;
        // First attempt 503, second 204: the client retries and succeeds.
        Mock::given(method("DELETE"))
            .and(path("/upload/files/f-2"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/upload/files/f-2"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let mut client = StorageClient::new(
            server.uri(),
            format!("{}/oauth/token", server.uri()),
            "cid",
            "csecret",
        );
        assert!(client.delete_file("f-2").await.is_ok());
    }

    #[tokio::test]
    async fn test_bad_token_endpoint_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let mut client = StorageClient::new(
            server.uri(),
            format!("{}/oauth/token", server.uri()),
            "cid",
            "csecret",
        );
        assert!(client.delete_file("f-3").await.is_err());
    }
}
