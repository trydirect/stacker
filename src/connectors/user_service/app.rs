use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::connectors::errors::ConnectorError;

use super::UserServiceClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    #[serde(rename = "_id")]
    pub id: Option<i64>,
    pub name: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub docker_image: Option<String>,
    pub default_port: Option<i32>,
    /// Ansible role name for template rendering
    #[serde(default)]
    pub role: Option<String>,
    /// Default environment variables from app_var table
    #[serde(default)]
    pub default_env: Option<serde_json::Value>,
    /// Default ports configuration from app table
    #[serde(default)]
    pub default_ports: Option<serde_json::Value>,
    /// Default config file templates from app_var (with attachment_path)
    #[serde(default)]
    pub default_config_files: Option<serde_json::Value>,
}

// Wrapper types for Eve-style responses
#[derive(Debug, Deserialize)]
struct ApplicationsResponse {
    _items: Vec<Application>,
}

impl UserServiceClient {
    /// Search available applications/stacks
    pub async fn search_applications(
        &self,
        bearer_token: &str,
        query: Option<&str>,
    ) -> Result<Vec<Application>, ConnectorError> {
        let url = format!("{}/applications", self.base_url);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .send()
            .await
            .map_err(ConnectorError::from)?;

        if response.status() == StatusCode::NOT_FOUND {
            return self.search_stack_view(bearer_token, query).await;
        }

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(ConnectorError::HttpError(format!(
                "User Service error ({}): {}",
                status, body
            )));
        }

        // User Service returns { "_items": [...], "_meta": {...} }
        let wrapper: ApplicationsResponse = response
            .json()
            .await
            .map_err(|e| ConnectorError::InvalidResponse(e.to_string()))?;
        let mut apps = wrapper._items;

        if let Some(q) = query {
            let q = q.to_lowercase();
            apps.retain(|app| {
                let name = app.name.as_deref().unwrap_or("").to_lowercase();
                let code = app.code.as_deref().unwrap_or("").to_lowercase();
                name.contains(&q) || code.contains(&q)
            });
        }

        Ok(apps)
    }

    /// Fetch enriched app catalog data from /applications/catalog endpoint.
    /// Returns apps with correct Docker images and default env/config from app + app_var tables.
    /// Falls back to search_applications() if the catalog endpoint is not available.
    pub async fn fetch_app_catalog(
        &self,
        bearer_token: &str,
        code: &str,
    ) -> Result<Option<Application>, ConnectorError> {
        let url = format!(
            "{}/applications/catalog/{}",
            self.base_url,
            urlencoding::encode(code)
        );

        tracing::info!("Fetching app catalog for code={} from {}", code, url);

        let response = match self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                tracing::warn!(
                    "Catalog endpoint transport error for code={}: {}, falling back to search_applications",
                    code, e
                );
                return self.fallback_search_by_code(bearer_token, code).await;
            }
        };

        if response.status() == StatusCode::NOT_FOUND {
            tracing::info!(
                "Catalog endpoint returned 404 for code={}, falling back to search_applications",
                code
            );
            return self.fallback_search_by_code(bearer_token, code).await;
        }

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!(
                "Catalog endpoint error ({}) for code={}: {}, falling back to search_applications",
                status, code, body
            );
            return self.fallback_search_by_code(bearer_token, code).await;
        }

        match response.json::<Application>().await {
            Ok(app) => Ok(Some(app)),
            Err(e) => {
                tracing::warn!(
                    "Catalog endpoint response parse error for code={}: {}, falling back to search_applications",
                    code, e
                );
                self.fallback_search_by_code(bearer_token, code).await
            }
        }
    }

    /// Helper: fall back to search_applications and find by exact code match.
    async fn fallback_search_by_code(
        &self,
        bearer_token: &str,
        code: &str,
    ) -> Result<Option<Application>, ConnectorError> {
        let apps = self.search_applications(bearer_token, Some(code)).await?;
        let code_lower = code.to_lowercase();
        Ok(apps.into_iter().find(|app| {
            app.code
                .as_deref()
                .map(|c| c.to_lowercase() == code_lower)
                .unwrap_or(false)
        }))
    }
}
