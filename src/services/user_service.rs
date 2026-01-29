//! User Service HTTP client for proxying requests to TryDirect User Service.
//!
//! This module provides typed access to User Service endpoints for:
//! - User profile information
//! - Subscription plans and limits
//! - Installations/deployments
//! - Applications catalog

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const REQUEST_TIMEOUT_SECS: u64 = 10;

/// HTTP client for User Service API
#[derive(Clone)]
pub struct UserServiceClient {
    base_url: String,
    client: Client,
}

impl UserServiceClient {
    /// Create a new User Service client
    pub fn new(base_url: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    /// Get current user profile
    pub async fn get_user_profile(
        &self,
        bearer_token: &str,
    ) -> Result<UserProfile, UserServiceError> {
        let url = format!("{}/auth/me", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .send()
            .await
            .map_err(|e| UserServiceError::Request(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(UserServiceError::Api {
                status,
                message: body,
            });
        }

        response
            .json::<UserProfile>()
            .await
            .map_err(|e| UserServiceError::Parse(e.to_string()))
    }

    /// Get user's subscription plan and limits
    pub async fn get_subscription_plan(
        &self,
        bearer_token: &str,
    ) -> Result<SubscriptionPlan, UserServiceError> {
        // Use the /oauth_server/api/me endpoint which returns user profile including plan info
        let url = format!("{}/oauth_server/api/me", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .send()
            .await
            .map_err(|e| UserServiceError::Request(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(UserServiceError::Api {
                status,
                message: body,
            });
        }

        // The response includes the user profile with "plan" field
        let user_profile: serde_json::Value = response
            .json()
            .await
            .map_err(|e| UserServiceError::Parse(e.to_string()))?;

        // Extract the "plan" field from the user profile
        let plan_value = user_profile
            .get("plan")
            .ok_or_else(|| UserServiceError::Parse("No plan field in user profile".to_string()))?;

        serde_json::from_value(plan_value.clone())
            .map_err(|e| UserServiceError::Parse(format!("Failed to parse plan: {}", e)))
    }

    /// List user's installations (deployments)
    pub async fn list_installations(
        &self,
        bearer_token: &str,
    ) -> Result<Vec<Installation>, UserServiceError> {
        let url = format!("{}/installations", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .send()
            .await
            .map_err(|e| UserServiceError::Request(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(UserServiceError::Api {
                status,
                message: body,
            });
        }

        // User Service returns { "_items": [...], "_meta": {...} }
        let wrapper: InstallationsResponse = response
            .json()
            .await
            .map_err(|e| UserServiceError::Parse(e.to_string()))?;

        Ok(wrapper._items)
    }

    /// Get specific installation details
    pub async fn get_installation(
        &self,
        bearer_token: &str,
        installation_id: i64,
    ) -> Result<InstallationDetails, UserServiceError> {
        let url = format!("{}/installations/{}", self.base_url, installation_id);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .send()
            .await
            .map_err(|e| UserServiceError::Request(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(UserServiceError::Api {
                status,
                message: body,
            });
        }

        response
            .json::<InstallationDetails>()
            .await
            .map_err(|e| UserServiceError::Parse(e.to_string()))
    }

    /// Search available applications/stacks
    pub async fn search_applications(
        &self,
        bearer_token: &str,
        query: Option<&str>,
    ) -> Result<Vec<Application>, UserServiceError> {
        let mut url = format!("{}/applications", self.base_url);
        if let Some(q) = query {
            url = format!("{}?where={{\"name\":{{\"{}\"}}}}", url, q);
        }

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .send()
            .await
            .map_err(|e| UserServiceError::Request(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(UserServiceError::Api {
                status,
                message: body,
            });
        }

        // User Service returns { "_items": [...], "_meta": {...} }
        let wrapper: ApplicationsResponse = response
            .json()
            .await
            .map_err(|e| UserServiceError::Parse(e.to_string()))?;

        Ok(wrapper._items)
    }
}

/// Error types for User Service operations
#[derive(Debug)]
pub enum UserServiceError {
    Request(String),
    Api { status: u16, message: String },
    Parse(String),
}

impl std::fmt::Display for UserServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserServiceError::Request(msg) => write!(f, "Request error: {}", msg),
            UserServiceError::Api { status, message } => {
                write!(f, "API error ({}): {}", status, message)
            }
            UserServiceError::Parse(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for UserServiceError {}

// Response types from User Service

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    #[serde(rename = "_id")]
    pub id: Option<String>,
    pub email: Option<String>,
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    pub roles: Option<Vec<String>>,
    #[serde(rename = "_created")]
    pub created_at: Option<String>,
    #[serde(rename = "_updated")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionPlan {
    /// Plan name (e.g., "Free", "Basic", "Plus")
    pub name: Option<String>,

    /// Plan code (e.g., "plan-free-periodically", "plan-basic-monthly")
    pub code: Option<String>,

    /// Plan features and limits (array of strings)
    pub includes: Option<Vec<String>>,

    /// Expiration date (null for active subscriptions)
    pub date_end: Option<String>,

    /// Whether the plan is active (date_end is null)
    pub active: Option<bool>,

    /// Price of the plan
    pub price: Option<String>,

    /// Currency (e.g., "USD")
    pub currency: Option<String>,

    /// Billing period ("month" or "year")
    pub period: Option<String>,

    /// Date of purchase
    pub date_of_purchase: Option<String>,

    /// Billing agreement ID
    pub billing_id: Option<String>,
}

// Note: PlanLimits struct is not currently used as limits come from the "includes" field
// which is an array of strings. Uncomment if structured limits are needed in the future.
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct PlanLimits {
//     pub max_deployments: Option<i32>,
//     pub max_apps_per_deployment: Option<i32>,
//     pub max_storage_gb: Option<i32>,
//     pub max_bandwidth_gb: Option<i32>,
// }

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
struct InstallationsResponse {
    _items: Vec<Installation>,
}

#[derive(Debug, Deserialize)]
struct ApplicationsResponse {
    _items: Vec<Application>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = UserServiceClient::new("http://localhost:4100");
        assert_eq!(client.base_url, "http://localhost:4100");
    }

    #[test]
    fn test_url_trailing_slash() {
        let client = UserServiceClient::new("http://localhost:4100/");
        assert_eq!(client.base_url, "http://localhost:4100");
    }
}
