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
}
