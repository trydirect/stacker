pub mod category_sync;
pub mod deployment_validator;
pub mod marketplace_webhook;

pub use category_sync::sync_categories_from_user_service;
pub use deployment_validator::{DeploymentValidationError, DeploymentValidator};
pub use marketplace_webhook::{
    MarketplaceWebhookPayload, MarketplaceWebhookSender, WebhookResponse, WebhookSenderConfig,
};

use super::config::UserServiceConfig;
use super::errors::ConnectorError;
use actix_web::web;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::Instrument;
use uuid::Uuid;

/// Response from User Service when creating a stack from marketplace template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackResponse {
    pub id: i32,
    pub user_id: String,
    pub name: String,
    pub marketplace_template_id: Option<Uuid>,
    pub is_from_marketplace: bool,
    pub template_version: Option<String>,
}

/// User's current plan information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPlanInfo {
    pub user_id: String,
    pub plan_name: String,
    pub plan_description: Option<String>,
    pub tier: Option<String>,
    pub active: bool,
    pub started_at: Option<String>,
    pub expires_at: Option<String>,
}

/// Available plan definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanDefinition {
    pub name: String,
    pub description: Option<String>,
    pub tier: Option<String>,
    pub features: Option<serde_json::Value>,
}

/// Product owned by a user (from /oauth_server/api/me response)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProduct {
    pub id: Option<String>,
    pub name: String,
    pub code: String,
    pub product_type: String,
    #[serde(default)]
    pub external_id: Option<i32>, // Stack template ID from Stacker
    #[serde(default)]
    pub owned_since: Option<String>,
}

/// User profile with ownership information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub email: String,
    pub plan: Option<serde_json::Value>, // Plan details from existing endpoint
    #[serde(default)]
    pub products: Vec<UserProduct>, // List of owned products
}

/// Product information from User Service catalog
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductInfo {
    pub id: String,
    pub name: String,
    pub code: String,
    pub product_type: String,
    pub external_id: Option<i32>,
    pub price: Option<f64>,
    pub billing_cycle: Option<String>,
    pub currency: Option<String>,
    pub vendor_id: Option<i32>,
    pub is_active: bool,
}

/// Category information from User Service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryInfo {
    #[serde(rename = "_id")]
    pub id: i32,
    pub name: String,
    pub title: String,
    #[serde(default)]
    pub priority: Option<i32>,
}

/// Trait for User Service integration
/// Allows mocking in tests and swapping implementations
#[async_trait::async_trait]
pub trait UserServiceConnector: Send + Sync {
    /// Create a new stack in User Service from a marketplace template
    async fn create_stack_from_template(
        &self,
        marketplace_template_id: &Uuid,
        user_id: &str,
        template_version: &str,
        name: &str,
        stack_definition: serde_json::Value,
    ) -> Result<StackResponse, ConnectorError>;

    /// Fetch stack details from User Service
    async fn get_stack(
        &self,
        stack_id: i32,
        user_id: &str,
    ) -> Result<StackResponse, ConnectorError>;

    /// List user's stacks
    async fn list_stacks(&self, user_id: &str) -> Result<Vec<StackResponse>, ConnectorError>;

    /// Check if user has access to a specific plan
    /// Returns true if user's current plan allows access to required_plan_name
    async fn user_has_plan(
        &self,
        user_id: &str,
        required_plan_name: &str,
    ) -> Result<bool, ConnectorError>;

    /// Get user's current plan information
    async fn get_user_plan(&self, user_id: &str) -> Result<UserPlanInfo, ConnectorError>;

    /// List all available plans that users can subscribe to
    async fn list_available_plans(&self) -> Result<Vec<PlanDefinition>, ConnectorError>;

    /// Get user profile with owned products list
    /// Calls GET /oauth_server/api/me and returns profile with products array
    async fn get_user_profile(&self, user_token: &str) -> Result<UserProfile, ConnectorError>;

    /// Get product information for a marketplace template
    /// Calls GET /api/1.0/products?external_id={template_id}&product_type=template
    async fn get_template_product(
        &self,
        stack_template_id: i32,
    ) -> Result<Option<ProductInfo>, ConnectorError>;

    /// Check if user owns a specific template product
    /// Returns true if user has the template in their products list
    async fn user_owns_template(
        &self,
        user_token: &str,
        stack_template_id: &str,
    ) -> Result<bool, ConnectorError>;

    /// Get list of categories from User Service
    /// Calls GET /api/1.0/category and returns available categories
    async fn get_categories(&self) -> Result<Vec<CategoryInfo>, ConnectorError>;
}

/// HTTP-based User Service client
pub struct UserServiceClient {
    base_url: String,
    http_client: reqwest::Client,
    auth_token: Option<String>,
    retry_attempts: usize,
}

impl UserServiceClient {
    /// Create new User Service client
    pub fn new(config: UserServiceConfig) -> Self {
        let timeout = std::time::Duration::from_secs(config.timeout_secs);
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            base_url: config.base_url,
            http_client,
            auth_token: config.auth_token,
            retry_attempts: config.retry_attempts,
        }
    }

    /// Build authorization header if token configured
    fn auth_header(&self) -> Option<String> {
        self.auth_token
            .as_ref()
            .map(|token| format!("Bearer {}", token))
    }

    /// Retry helper with exponential backoff
    async fn retry_request<F, T>(&self, mut f: F) -> Result<T, ConnectorError>
    where
        F: FnMut() -> futures::future::BoxFuture<'static, Result<T, ConnectorError>>,
    {
        let mut attempt = 0;
        loop {
            match f().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    attempt += 1;
                    if attempt >= self.retry_attempts {
                        return Err(err);
                    }
                    // Exponential backoff: 100ms, 200ms, 400ms, etc.
                    let backoff = std::time::Duration::from_millis(100 * 2_u64.pow(attempt as u32));
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl UserServiceConnector for UserServiceClient {
    async fn create_stack_from_template(
        &self,
        marketplace_template_id: &Uuid,
        user_id: &str,
        template_version: &str,
        name: &str,
        stack_definition: serde_json::Value,
    ) -> Result<StackResponse, ConnectorError> {
        let span = tracing::info_span!(
            "user_service_create_stack",
            template_id = %marketplace_template_id,
            user_id = %user_id
        );

        let url = format!("{}/api/1.0/stacks", self.base_url);
        let payload = serde_json::json!({
            "name": name,
            "marketplace_template_id": marketplace_template_id.to_string(),
            "is_from_marketplace": true,
            "template_version": template_version,
            "stack_definition": stack_definition,
            "user_id": user_id,
        });

        let mut req = self.http_client.post(&url).json(&payload);

        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let resp = req
            .send()
            .instrument(span)
            .await
            .and_then(|resp| resp.error_for_status())
            .map_err(|e| {
                tracing::error!("create_stack error: {:?}", e);
                ConnectorError::HttpError(format!("Failed to create stack: {}", e))
            })?;

        let text = resp
            .text()
            .await
            .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
        serde_json::from_str::<StackResponse>(&text)
            .map_err(|_| ConnectorError::InvalidResponse(text))
    }

    async fn get_stack(
        &self,
        stack_id: i32,
        user_id: &str,
    ) -> Result<StackResponse, ConnectorError> {
        let span =
            tracing::info_span!("user_service_get_stack", stack_id = stack_id, user_id = %user_id);

        let url = format!("{}/api/1.0/stacks/{}", self.base_url, stack_id);
        let mut req = self.http_client.get(&url);

        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let resp = req.send().instrument(span).await.map_err(|e| {
            if e.status().map_or(false, |s| s == 404) {
                ConnectorError::NotFound(format!("Stack {} not found", stack_id))
            } else {
                ConnectorError::HttpError(format!("Failed to get stack: {}", e))
            }
        })?;

        if resp.status() == 404 {
            return Err(ConnectorError::NotFound(format!(
                "Stack {} not found",
                stack_id
            )));
        }

        let text = resp
            .text()
            .await
            .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
        serde_json::from_str::<StackResponse>(&text)
            .map_err(|_| ConnectorError::InvalidResponse(text))
    }

    async fn list_stacks(&self, user_id: &str) -> Result<Vec<StackResponse>, ConnectorError> {
        let span = tracing::info_span!("user_service_list_stacks", user_id = %user_id);

        let url = format!(
            "{}/api/1.0/stacks?where={{\"user_id\":\"{}\"}}",
            self.base_url, user_id
        );
        let mut req = self.http_client.get(&url);

        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        #[derive(Deserialize)]
        struct ListResponse {
            _items: Vec<StackResponse>,
        }

        let resp = req
            .send()
            .instrument(span)
            .await
            .and_then(|resp| resp.error_for_status())
            .map_err(|e| {
                tracing::error!("list_stacks error: {:?}", e);
                ConnectorError::HttpError(format!("Failed to list stacks: {}", e))
            })?;

        let text = resp
            .text()
            .await
            .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
        serde_json::from_str::<ListResponse>(&text)
            .map(|r| r._items)
            .map_err(|_| ConnectorError::InvalidResponse(text))
    }

    async fn user_has_plan(
        &self,
        user_id: &str,
        required_plan_name: &str,
    ) -> Result<bool, ConnectorError> {
        let span = tracing::info_span!(
            "user_service_check_plan",
            user_id = %user_id,
            required_plan = %required_plan_name
        );

        // Get user's current plan via /oauth_server/api/me endpoint
        let url = format!("{}/oauth_server/api/me", self.base_url);
        let mut req = self.http_client.get(&url);

        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        #[derive(serde::Deserialize)]
        struct UserMeResponse {
            #[serde(default)]
            plan: Option<PlanInfo>,
        }

        #[derive(serde::Deserialize)]
        struct PlanInfo {
            name: Option<String>,
        }

        let resp = req.send().instrument(span.clone()).await.map_err(|e| {
            tracing::error!("user_has_plan error: {:?}", e);
            ConnectorError::HttpError(format!("Failed to check plan: {}", e))
        })?;

        match resp.status().as_u16() {
            200 => {
                let text = resp
                    .text()
                    .await
                    .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
                serde_json::from_str::<UserMeResponse>(&text)
                    .map(|response| {
                        let user_plan = response.plan.and_then(|p| p.name).unwrap_or_default();
                        // Check if user's plan matches or is higher tier than required
                        if user_plan.is_empty() || required_plan_name.is_empty() {
                            return user_plan == required_plan_name;
                        }
                        user_plan == required_plan_name
                            || is_plan_upgrade(&user_plan, required_plan_name)
                    })
                    .map_err(|_| ConnectorError::InvalidResponse(text))
            }
            401 | 403 => {
                tracing::debug!(parent: &span, "User not authenticated or authorized");
                Ok(false)
            }
            404 => {
                tracing::debug!(parent: &span, "User or plan not found");
                Ok(false)
            }
            _ => Err(ConnectorError::HttpError(format!(
                "Unexpected status code: {}",
                resp.status()
            ))),
        }
    }

    async fn get_user_plan(&self, user_id: &str) -> Result<UserPlanInfo, ConnectorError> {
        let span = tracing::info_span!("user_service_get_plan", user_id = %user_id);

        // Use /oauth_server/api/me endpoint to get user's current plan via OAuth
        let url = format!("{}/oauth_server/api/me", self.base_url);
        let mut req = self.http_client.get(&url);

        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        #[derive(serde::Deserialize)]
        struct PlanInfoResponse {
            #[serde(default)]
            plan: Option<String>,
            #[serde(default)]
            plan_name: Option<String>,
            #[serde(default)]
            user_id: Option<String>,
            #[serde(default)]
            description: Option<String>,
            #[serde(default)]
            active: Option<bool>,
        }

        let resp = req
            .send()
            .instrument(span)
            .await
            .and_then(|resp| resp.error_for_status())
            .map_err(|e| {
                tracing::error!("get_user_plan error: {:?}", e);
                ConnectorError::HttpError(format!("Failed to get user plan: {}", e))
            })?;

        let text = resp
            .text()
            .await
            .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
        serde_json::from_str::<PlanInfoResponse>(&text)
            .map(|info| UserPlanInfo {
                user_id: info.user_id.unwrap_or_else(|| user_id.to_string()),
                plan_name: info.plan.or(info.plan_name).unwrap_or_default(),
                plan_description: info.description,
                tier: None,
                active: info.active.unwrap_or(true),
                started_at: None,
                expires_at: None,
            })
            .map_err(|_| ConnectorError::InvalidResponse(text))
    }

    async fn list_available_plans(&self) -> Result<Vec<PlanDefinition>, ConnectorError> {
        let span = tracing::info_span!("user_service_list_plans");

        // Query plan_description via Eve REST API (PostgREST endpoint)
        let url = format!("{}/api/1.0/plan_description", self.base_url);
        let mut req = self.http_client.get(&url);

        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        #[derive(serde::Deserialize)]
        struct EveResponse {
            #[serde(default)]
            _items: Vec<PlanDefinition>,
        }

        #[derive(serde::Deserialize)]
        struct PlanItem {
            name: String,
            #[serde(default)]
            description: Option<String>,
            #[serde(default)]
            tier: Option<String>,
            #[serde(default)]
            features: Option<serde_json::Value>,
        }

        let resp = req
            .send()
            .instrument(span)
            .await
            .and_then(|resp| resp.error_for_status())
            .map_err(|e| {
                tracing::error!("list_available_plans error: {:?}", e);
                ConnectorError::HttpError(format!("Failed to list plans: {}", e))
            })?;

        let text = resp
            .text()
            .await
            .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

        // Try Eve format first, fallback to direct array
        if let Ok(eve_resp) = serde_json::from_str::<EveResponse>(&text) {
            Ok(eve_resp._items)
        } else {
            serde_json::from_str::<Vec<PlanDefinition>>(&text)
                .map_err(|_| ConnectorError::InvalidResponse(text))
        }
    }

    async fn get_user_profile(&self, user_token: &str) -> Result<UserProfile, ConnectorError> {
        let span = tracing::info_span!("user_service_get_profile");

        // Query /oauth_server/api/me with user's token
        let url = format!("{}/oauth_server/api/me", self.base_url);
        let req = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", user_token));

        let resp = req.send().instrument(span.clone()).await.map_err(|e| {
            tracing::error!("get_user_profile error: {:?}", e);
            ConnectorError::HttpError(format!("Failed to get user profile: {}", e))
        })?;

        if resp.status() == 401 {
            return Err(ConnectorError::Unauthorized(
                "Invalid or expired user token".to_string(),
            ));
        }

        let text = resp
            .text()
            .await
            .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
        serde_json::from_str::<UserProfile>(&text).map_err(|e| {
            tracing::error!("Failed to parse user profile: {:?}", e);
            ConnectorError::InvalidResponse(text)
        })
    }

    async fn get_template_product(
        &self,
        stack_template_id: i32,
    ) -> Result<Option<ProductInfo>, ConnectorError> {
        let span = tracing::info_span!(
            "user_service_get_template_product",
            template_id = stack_template_id
        );

        // Query /api/1.0/products?external_id={template_id}&product_type=template
        let url = format!(
            "{}/api/1.0/products?where={{\"external_id\":{},\"product_type\":\"template\"}}",
            self.base_url, stack_template_id
        );

        let mut req = self.http_client.get(&url);

        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        #[derive(serde::Deserialize)]
        struct ProductsResponse {
            #[serde(default)]
            _items: Vec<ProductInfo>,
        }

        let resp = req.send().instrument(span).await.map_err(|e| {
            tracing::error!("get_template_product error: {:?}", e);
            ConnectorError::HttpError(format!("Failed to get template product: {}", e))
        })?;

        let text = resp
            .text()
            .await
            .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

        // Try Eve format first (with _items wrapper)
        if let Ok(products_resp) = serde_json::from_str::<ProductsResponse>(&text) {
            Ok(products_resp._items.into_iter().next())
        } else {
            // Try direct array format
            serde_json::from_str::<Vec<ProductInfo>>(&text)
                .map(|mut items| items.pop())
                .map_err(|_| ConnectorError::InvalidResponse(text))
        }
    }

    async fn user_owns_template(
        &self,
        user_token: &str,
        stack_template_id: &str,
    ) -> Result<bool, ConnectorError> {
        let span = tracing::info_span!(
            "user_service_check_template_ownership",
            template_id = stack_template_id
        );

        // Get user profile (includes products list)
        let profile = self
            .get_user_profile(user_token)
            .instrument(span.clone())
            .await?;

        // Try to parse stack_template_id as i32 first (for backward compatibility with integer IDs)
        let owns_template = if let Ok(template_id_int) = stack_template_id.parse::<i32>() {
            profile
                .products
                .iter()
                .any(|p| p.product_type == "template" && p.external_id == Some(template_id_int))
        } else {
            // If not i32, try comparing as string (UUID or slug)
            profile.products.iter().any(|p| {
                if p.product_type != "template" {
                    return false;
                }
                // Compare with code (slug)
                if p.code == stack_template_id {
                    return true;
                }
                // Compare with id if available
                if let Some(id) = &p.id {
                    if id == stack_template_id {
                        return true;
                    }
                }
                false
            })
        };

        tracing::info!(
            owned = owns_template,
            "User template ownership check complete"
        );

        Ok(owns_template)
    }

    async fn get_categories(&self) -> Result<Vec<CategoryInfo>, ConnectorError> {
        let span = tracing::info_span!("user_service_get_categories");
        let url = format!("{}/api/1.0/category", self.base_url);

        let mut attempt = 0;
        loop {
            attempt += 1;

            let mut req = self.http_client.get(&url);

            if let Some(auth) = self.auth_header() {
                req = req.header("Authorization", auth);
            }

            match req.send().instrument(span.clone()).await {
                Ok(resp) => match resp.status().as_u16() {
                    200 => {
                        let text = resp
                            .text()
                            .await
                            .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

                        // User Service returns {_items: [...]}
                        #[derive(Deserialize)]
                        struct CategoriesResponse {
                            #[serde(rename = "_items")]
                            items: Vec<CategoryInfo>,
                        }

                        return serde_json::from_str::<CategoriesResponse>(&text)
                            .map(|resp| resp.items)
                            .map_err(|e| {
                                tracing::error!("Failed to parse categories response: {:?}", e);
                                ConnectorError::InvalidResponse(text)
                            });
                    }
                    404 => {
                        return Err(ConnectorError::NotFound(
                            "Category endpoint not found".to_string(),
                        ));
                    }
                    500..=599 => {
                        if attempt < self.retry_attempts {
                            let backoff = std::time::Duration::from_millis(
                                100 * 2_u64.pow((attempt - 1) as u32),
                            );
                            tracing::warn!(
                                "User Service categories request failed with {}, retrying after {:?}",
                                resp.status(),
                                backoff
                            );
                            tokio::time::sleep(backoff).await;
                            continue;
                        }
                        return Err(ConnectorError::ServiceUnavailable(format!(
                            "User Service returned {}: get categories failed",
                            resp.status()
                        )));
                    }
                    status => {
                        return Err(ConnectorError::HttpError(format!(
                            "Unexpected status code: {}",
                            status
                        )));
                    }
                },
                Err(e) if e.is_timeout() => {
                    if attempt < self.retry_attempts {
                        let backoff =
                            std::time::Duration::from_millis(100 * 2_u64.pow((attempt - 1) as u32));
                        tracing::warn!(
                            "User Service get categories timeout, retrying after {:?}",
                            backoff
                        );
                        tokio::time::sleep(backoff).await;
                        continue;
                    }
                    return Err(ConnectorError::ServiceUnavailable(
                        "Get categories timeout".to_string(),
                    ));
                }
                Err(e) => {
                    return Err(ConnectorError::HttpError(format!(
                        "Get categories request failed: {}",
                        e
                    )));
                }
            }
        }
    }
}

/// Mock connector for testing/development
pub mod mock {
    use super::*;

    /// Mock User Service for testing - always succeeds
    pub struct MockUserServiceConnector;

    #[async_trait::async_trait]
    impl UserServiceConnector for MockUserServiceConnector {
        async fn create_stack_from_template(
            &self,
            marketplace_template_id: &Uuid,
            user_id: &str,
            template_version: &str,
            name: &str,
            _stack_definition: serde_json::Value,
        ) -> Result<StackResponse, ConnectorError> {
            Ok(StackResponse {
                id: 1,
                user_id: user_id.to_string(),
                name: name.to_string(),
                marketplace_template_id: Some(*marketplace_template_id),
                is_from_marketplace: true,
                template_version: Some(template_version.to_string()),
            })
        }

        async fn get_stack(
            &self,
            stack_id: i32,
            user_id: &str,
        ) -> Result<StackResponse, ConnectorError> {
            Ok(StackResponse {
                id: stack_id,
                user_id: user_id.to_string(),
                name: "Test Stack".to_string(),
                marketplace_template_id: None,
                is_from_marketplace: false,
                template_version: None,
            })
        }

        async fn list_stacks(&self, user_id: &str) -> Result<Vec<StackResponse>, ConnectorError> {
            Ok(vec![StackResponse {
                id: 1,
                user_id: user_id.to_string(),
                name: "Test Stack".to_string(),
                marketplace_template_id: None,
                is_from_marketplace: false,
                template_version: None,
            }])
        }

        async fn user_has_plan(
            &self,
            _user_id: &str,
            _required_plan_name: &str,
        ) -> Result<bool, ConnectorError> {
            // Mock always grants access for testing
            Ok(true)
        }

        async fn get_user_plan(&self, user_id: &str) -> Result<UserPlanInfo, ConnectorError> {
            Ok(UserPlanInfo {
                user_id: user_id.to_string(),
                plan_name: "professional".to_string(),
                plan_description: Some("Professional Plan".to_string()),
                tier: Some("pro".to_string()),
                active: true,
                started_at: Some("2025-01-01T00:00:00Z".to_string()),
                expires_at: None,
            })
        }

        async fn list_available_plans(&self) -> Result<Vec<PlanDefinition>, ConnectorError> {
            Ok(vec![
                PlanDefinition {
                    name: "basic".to_string(),
                    description: Some("Basic Plan".to_string()),
                    tier: Some("basic".to_string()),
                    features: None,
                },
                PlanDefinition {
                    name: "professional".to_string(),
                    description: Some("Professional Plan".to_string()),
                    tier: Some("pro".to_string()),
                    features: None,
                },
                PlanDefinition {
                    name: "enterprise".to_string(),
                    description: Some("Enterprise Plan".to_string()),
                    tier: Some("enterprise".to_string()),
                    features: None,
                },
            ])
        }

        async fn get_user_profile(&self, _user_token: &str) -> Result<UserProfile, ConnectorError> {
            Ok(UserProfile {
                email: "test@example.com".to_string(),
                plan: Some(serde_json::json!({
                    "name": "professional",
                    "date_end": "2026-12-31"
                })),
                products: vec![
                    UserProduct {
                        id: Some("uuid-plan-pro".to_string()),
                        name: "Professional Plan".to_string(),
                        code: "professional".to_string(),
                        product_type: "plan".to_string(),
                        external_id: None,
                        owned_since: Some("2025-01-01T00:00:00Z".to_string()),
                    },
                    UserProduct {
                        id: Some("uuid-template-ai".to_string()),
                        name: "AI Agent Stack Pro".to_string(),
                        code: "ai-agent-stack-pro".to_string(),
                        product_type: "template".to_string(),
                        external_id: Some(100), // Mock template ID
                        owned_since: Some("2025-01-15T00:00:00Z".to_string()),
                    },
                ],
            })
        }

        async fn get_template_product(
            &self,
            stack_template_id: i32,
        ) -> Result<Option<ProductInfo>, ConnectorError> {
            // Return mock product only if template_id is our test ID
            if stack_template_id == 100 {
                Ok(Some(ProductInfo {
                    id: "uuid-product-ai".to_string(),
                    name: "AI Agent Stack Pro".to_string(),
                    code: "ai-agent-stack-pro".to_string(),
                    product_type: "template".to_string(),
                    external_id: Some(100),
                    price: Some(99.99),
                    billing_cycle: Some("one_time".to_string()),
                    currency: Some("USD".to_string()),
                    vendor_id: Some(456),
                    is_active: true,
                }))
            } else {
                Ok(None) // No product for other template IDs
            }
        }

        async fn user_owns_template(
            &self,
            _user_token: &str,
            stack_template_id: &str,
        ) -> Result<bool, ConnectorError> {
            // Mock user owns template if ID is "100" or contains "ai-agent"
            Ok(stack_template_id == "100" || stack_template_id.contains("ai-agent"))
        }

        async fn get_categories(&self) -> Result<Vec<CategoryInfo>, ConnectorError> {
            // Return mock categories
            Ok(vec![
                CategoryInfo {
                    id: 1,
                    name: "cms".to_string(),
                    title: "CMS".to_string(),
                    priority: Some(1),
                },
                CategoryInfo {
                    id: 2,
                    name: "ecommerce".to_string(),
                    title: "E-commerce".to_string(),
                    priority: Some(2),
                },
                CategoryInfo {
                    id: 5,
                    name: "ai".to_string(),
                    title: "AI Agents".to_string(),
                    priority: Some(5),
                },
            ])
        }
    }
}

/// Initialize User Service connector with config from Settings
///
/// Returns configured connector wrapped in web::Data for injection into Actix app
/// Also spawns background task to sync categories from User Service
///
/// # Example
/// ```ignore
/// // In startup.rs
/// let user_service = connectors::user_service::init(&settings.connectors, pg_pool.clone());
/// App::new().app_data(user_service)
/// ```
pub fn init(
    connector_config: &super::config::ConnectorConfig,
    pg_pool: web::Data<sqlx::PgPool>,
) -> web::Data<Arc<dyn UserServiceConnector>> {
    let connector: Arc<dyn UserServiceConnector> = if let Some(user_service_config) =
        connector_config.user_service.as_ref().filter(|c| c.enabled)
    {
        let mut config = user_service_config.clone();
        // Load auth token from environment if not set in config
        if config.auth_token.is_none() {
            config.auth_token = std::env::var("USER_SERVICE_AUTH_TOKEN").ok();
        }
        tracing::info!("Initializing User Service connector: {}", config.base_url);
        Arc::new(UserServiceClient::new(config))
    } else {
        tracing::warn!("User Service connector disabled - using mock");
        Arc::new(mock::MockUserServiceConnector)
    };

    // Spawn background task to sync categories on startup
    let connector_clone = connector.clone();
    let pg_pool_clone = pg_pool.clone();
    tokio::spawn(async move {
        match connector_clone.get_categories().await {
            Ok(categories) => {
                tracing::info!("Fetched {} categories from User Service", categories.len());
                match crate::db::marketplace::sync_categories(pg_pool_clone.get_ref(), categories)
                    .await
                {
                    Ok(count) => tracing::info!("Successfully synced {} categories", count),
                    Err(e) => tracing::error!("Failed to sync categories to database: {}", e),
                }
            }
            Err(e) => tracing::warn!(
                "Failed to fetch categories from User Service (will retry later): {:?}",
                e
            ),
        }
    });

    web::Data::new(connector)
}

/// Helper function to determine if a plan tier can access a required plan
/// Basic idea: enterprise >= professional >= basic
fn is_plan_upgrade(user_plan: &str, required_plan: &str) -> bool {
    let plan_hierarchy = vec!["basic", "professional", "enterprise"];

    let user_level = plan_hierarchy
        .iter()
        .position(|&p| p == user_plan)
        .unwrap_or(0);
    let required_level = plan_hierarchy
        .iter()
        .position(|&p| p == required_plan)
        .unwrap_or(0);

    user_level > required_level
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// Test that get_user_profile returns user with products list
    #[tokio::test]
    async fn test_mock_get_user_profile_returns_user_with_products() {
        let connector = mock::MockUserServiceConnector;
        let profile = connector.get_user_profile("test_token").await.unwrap();

        // Assertions on user profile structure
        assert_eq!(profile.email, "test@example.com");
        assert!(profile.plan.is_some());

        // Verify products list is populated
        assert!(!profile.products.is_empty());

        // Check for plan product
        let plan_product = profile.products.iter().find(|p| p.product_type == "plan");
        assert!(plan_product.is_some());
        assert_eq!(plan_product.unwrap().code, "professional");

        // Check for template product
        let template_product = profile
            .products
            .iter()
            .find(|p| p.product_type == "template");
        assert!(template_product.is_some());
        assert_eq!(template_product.unwrap().name, "AI Agent Stack Pro");
        assert_eq!(template_product.unwrap().external_id, Some(100));
    }

    /// Test that get_template_product returns product info for owned templates
    #[tokio::test]
    async fn test_mock_get_template_product_returns_product_info() {
        let connector = mock::MockUserServiceConnector;

        // Test with template ID that exists (100)
        let product = connector.get_template_product(100).await.unwrap();
        assert!(product.is_some());

        let prod = product.unwrap();
        assert_eq!(prod.id, "uuid-product-ai");
        assert_eq!(prod.name, "AI Agent Stack Pro");
        assert_eq!(prod.code, "ai-agent-stack-pro");
        assert_eq!(prod.product_type, "template");
        assert_eq!(prod.external_id, Some(100));
        assert_eq!(prod.price, Some(99.99));
        assert_eq!(prod.currency, Some("USD".to_string()));
        assert!(prod.is_active);
    }

    /// Test that get_template_product returns None for non-existent templates
    #[tokio::test]
    async fn test_mock_get_template_product_not_found() {
        let connector = mock::MockUserServiceConnector;

        // Test with non-existent template ID
        let product = connector.get_template_product(999).await.unwrap();
        assert!(product.is_none());
    }

    /// Test that user_owns_template correctly identifies owned templates
    #[tokio::test]
    async fn test_mock_user_owns_template_owned() {
        let connector = mock::MockUserServiceConnector;

        // Test with owned template ID
        let owns = connector
            .user_owns_template("test_token", "100")
            .await
            .unwrap();
        assert!(owns);

        // Test with code containing "ai-agent"
        let owns_code = connector
            .user_owns_template("test_token", "ai-agent-stack-pro")
            .await
            .unwrap();
        assert!(owns_code);
    }

    /// Test that user_owns_template returns false for non-owned templates
    #[tokio::test]
    async fn test_mock_user_owns_template_not_owned() {
        let connector = mock::MockUserServiceConnector;

        // Test with non-owned template ID
        let owns = connector
            .user_owns_template("test_token", "999")
            .await
            .unwrap();
        assert!(!owns);

        // Test with random code that doesn't match
        let owns_code = connector
            .user_owns_template("test_token", "random-template")
            .await
            .unwrap();
        assert!(!owns_code);
    }

    /// Test that user_has_plan always returns true in mock (for testing)
    #[tokio::test]
    async fn test_mock_user_has_plan() {
        let connector = mock::MockUserServiceConnector;

        let has_professional = connector
            .user_has_plan("user_123", "professional")
            .await
            .unwrap();
        assert!(has_professional);

        let has_enterprise = connector
            .user_has_plan("user_123", "enterprise")
            .await
            .unwrap();
        assert!(has_enterprise);

        let has_basic = connector.user_has_plan("user_123", "basic").await.unwrap();
        assert!(has_basic);
    }

    /// Test that get_user_plan returns correct plan info
    #[tokio::test]
    async fn test_mock_get_user_plan() {
        let connector = mock::MockUserServiceConnector;

        let plan = connector.get_user_plan("user_123").await.unwrap();
        assert_eq!(plan.user_id, "user_123");
        assert_eq!(plan.plan_name, "professional");
        assert!(plan.plan_description.is_some());
        assert_eq!(plan.plan_description.unwrap(), "Professional Plan");
        assert!(plan.active);
    }

    /// Test that list_available_plans returns multiple plan definitions
    #[tokio::test]
    async fn test_mock_list_available_plans() {
        let connector = mock::MockUserServiceConnector;

        let plans = connector.list_available_plans().await.unwrap();
        assert!(!plans.is_empty());
        assert_eq!(plans.len(), 3);

        // Verify specific plans exist
        let plan_names: Vec<String> = plans.iter().map(|p| p.name.clone()).collect();
        assert!(plan_names.contains(&"basic".to_string()));
        assert!(plan_names.contains(&"professional".to_string()));
        assert!(plan_names.contains(&"enterprise".to_string()));
    }

    /// Test that get_categories returns category list
    #[tokio::test]
    async fn test_mock_get_categories() {
        let connector = mock::MockUserServiceConnector;

        let categories = connector.get_categories().await.unwrap();
        assert!(!categories.is_empty());
        assert_eq!(categories.len(), 3);

        // Verify specific categories exist
        let category_names: Vec<String> = categories.iter().map(|c| c.name.clone()).collect();
        assert!(category_names.contains(&"cms".to_string()));
        assert!(category_names.contains(&"ecommerce".to_string()));
        assert!(category_names.contains(&"ai".to_string()));

        // Verify category has expected fields
        let ai_category = categories.iter().find(|c| c.name == "ai").unwrap();
        assert_eq!(ai_category.title, "AI Agents");
        assert_eq!(ai_category.priority, Some(5));
    }

    /// Test that create_stack_from_template returns stack with marketplace info
    #[tokio::test]
    async fn test_mock_create_stack_from_template() {
        let connector = mock::MockUserServiceConnector;
        let template_id = Uuid::new_v4();

        let stack = connector
            .create_stack_from_template(
                &template_id,
                "user_123",
                "1.0.0",
                "My Stack",
                serde_json::json!({"services": []}),
            )
            .await
            .unwrap();

        assert_eq!(stack.user_id, "user_123");
        assert_eq!(stack.name, "My Stack");
        assert_eq!(stack.marketplace_template_id, Some(template_id));
        assert!(stack.is_from_marketplace);
        assert_eq!(stack.template_version, Some("1.0.0".to_string()));
    }

    /// Test that get_stack returns stack details
    #[tokio::test]
    async fn test_mock_get_stack() {
        let connector = mock::MockUserServiceConnector;

        let stack = connector.get_stack(1, "user_123").await.unwrap();
        assert_eq!(stack.id, 1);
        assert_eq!(stack.user_id, "user_123");
        assert_eq!(stack.name, "Test Stack");
    }

    /// Test that list_stacks returns user's stacks
    #[tokio::test]
    async fn test_mock_list_stacks() {
        let connector = mock::MockUserServiceConnector;

        let stacks = connector.list_stacks("user_123").await.unwrap();
        assert!(!stacks.is_empty());
        assert_eq!(stacks[0].user_id, "user_123");
    }

    /// Test plan hierarchy comparison
    #[test]
    fn test_is_plan_upgrade_hierarchy() {
        // Enterprise user can access professional tier
        assert!(is_plan_upgrade("enterprise", "professional"));

        // Enterprise user can access basic tier
        assert!(is_plan_upgrade("enterprise", "basic"));

        // Professional user can access basic tier
        assert!(is_plan_upgrade("professional", "basic"));

        // Basic user cannot access professional
        assert!(!is_plan_upgrade("basic", "professional"));

        // Basic user cannot access enterprise
        assert!(!is_plan_upgrade("basic", "enterprise"));

        // Same plan should not be considered upgrade
        assert!(!is_plan_upgrade("professional", "professional"));
    }

    /// Test UserProfile deserialization with all fields
    #[test]
    fn test_user_profile_deserialization() {
        let json = serde_json::json!({
            "email": "alice@example.com",
            "plan": {
                "name": "professional",
                "date_end": "2026-12-31"
            },
            "products": [
                {
                    "id": "prod-1",
                    "name": "Professional Plan",
                    "code": "professional",
                    "product_type": "plan",
                    "external_id": null,
                    "owned_since": "2025-01-01T00:00:00Z"
                },
                {
                    "id": "prod-2",
                    "name": "AI Stack",
                    "code": "ai-stack",
                    "product_type": "template",
                    "external_id": 42,
                    "owned_since": "2025-01-15T00:00:00Z"
                }
            ]
        });

        let profile: UserProfile = serde_json::from_value(json).unwrap();
        assert_eq!(profile.email, "alice@example.com");
        assert_eq!(profile.products.len(), 2);
        assert_eq!(profile.products[0].code, "professional");
        assert_eq!(profile.products[1].external_id, Some(42));
    }

    /// Test ProductInfo with optional fields
    #[test]
    fn test_product_info_deserialization() {
        let json = serde_json::json!({
            "id": "product-123",
            "name": "AI Stack Template",
            "code": "ai-stack-template",
            "product_type": "template",
            "external_id": 42,
            "price": 99.99,
            "billing_cycle": "one_time",
            "currency": "USD",
            "vendor_id": 123,
            "is_active": true
        });

        let product: ProductInfo = serde_json::from_value(json).unwrap();
        assert_eq!(product.id, "product-123");
        assert_eq!(product.price, Some(99.99));
        assert_eq!(product.external_id, Some(42));
        assert_eq!(product.currency, Some("USD".to_string()));
    }

    /// Test CategoryInfo deserialization
    #[test]
    fn test_category_info_deserialization() {
        let json = serde_json::json!({
            "_id": 5,
            "name": "ai",
            "title": "AI Agents",
            "priority": 5
        });

        let category: CategoryInfo = serde_json::from_value(json).unwrap();
        assert_eq!(category.id, 5);
        assert_eq!(category.name, "ai");
        assert_eq!(category.title, "AI Agents");
        assert_eq!(category.priority, Some(5));
    }
}
