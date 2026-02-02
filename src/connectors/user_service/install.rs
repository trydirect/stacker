use serde::{Deserialize, Serialize};

use crate::connectors::errors::ConnectorError;

use super::UserServiceClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installation {
    #[serde(rename = "_id")]
    pub id: Option<i64>,
    pub stack_code: Option<String>,
    pub status: Option<String>,
    pub cloud: Option<String>,
    pub deployment_hash: Option<String>,
    pub domain: Option<String>,
    #[serde(rename = "_created")]
    pub created_at: Option<String>,
    #[serde(rename = "_updated")]
    pub updated_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationDetails {
    #[serde(rename = "_id")]
    pub id: Option<i64>,
    pub stack_code: Option<String>,
    pub status: Option<String>,
    pub cloud: Option<String>,
    pub deployment_hash: Option<String>,
    pub domain: Option<String>,
    pub server_ip: Option<String>,
    pub apps: Option<Vec<InstallationApp>>,
    pub agent_config: Option<serde_json::Value>,
    #[serde(rename = "_created")]
    pub created_at: Option<String>,
    #[serde(rename = "_updated")]
    pub updated_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationApp {
    pub app_code: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub port: Option<i32>,
}

// Wrapper types for Eve-style responses
#[derive(Debug, Deserialize)]
struct InstallationsResponse {
    _items: Vec<Installation>,
}

impl UserServiceClient {
    /// List user's installations (deployments)
    pub async fn list_installations(
        &self,
        bearer_token: &str,
    ) -> Result<Vec<Installation>, ConnectorError> {
        let url = format!("{}/installations", self.base_url);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .send()
            .await
            .map_err(ConnectorError::from)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(ConnectorError::HttpError(format!(
                "User Service error ({}): {}",
                status, body
            )));
        }

        // User Service returns { "_items": [...], "_meta": {...} }
        let wrapper: InstallationsResponse = response
            .json()
            .await
            .map_err(|e| ConnectorError::InvalidResponse(e.to_string()))?;

        Ok(wrapper._items)
    }

    /// Get specific installation details
    pub async fn get_installation(
        &self,
        bearer_token: &str,
        installation_id: i64,
    ) -> Result<InstallationDetails, ConnectorError> {
        let url = format!("{}/installations/{}", self.base_url, installation_id);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .send()
            .await
            .map_err(ConnectorError::from)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(ConnectorError::HttpError(format!(
                "User Service error ({}): {}",
                status, body
            )));
        }

        response
            .json::<InstallationDetails>()
            .await
            .map_err(|e| ConnectorError::InvalidResponse(e.to_string()))
    }
}
