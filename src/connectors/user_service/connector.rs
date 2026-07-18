use uuid::Uuid;

use super::types::{
    AuthorizationHandle, BillingCapability, CategoryInfo, PlanDefinition, ProductInfo,
    StackResponse, UserPlanInfo, UserProfile,
};
use crate::connectors::errors::ConnectorError;

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
    /// Returns true if user's current plan allows access to required_plan_name.
    /// Pass `user_token` to authenticate as the user (preferred); falls back to
    /// the service account token when `None`.
    async fn user_has_plan(
        &self,
        user_id: &str,
        required_plan_name: &str,
        user_token: Option<&str>,
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

    /// Search the unified applications catalog used by try.direct/applications.
    async fn search_marketplace_templates(
        &self,
        user_token: &str,
        query: Option<&str>,
        category: Option<&str>,
        is_marketplace: Option<bool>,
        page: Option<u32>,
        max_results: Option<u32>,
    ) -> Result<Vec<serde_json::Value>, ConnectorError>;

    // ── Per-install billing (two-phase charge) ─────────────────────
    //
    // The four methods below implement the authorize/capture/void
    // dance used when a template's billing_cycle is "per_install".
    // user_service is authoritative on payment intents and idempotency;
    // stacker holds only the opaque `authorization_id` and a local
    // `marketplace_install_authorization` row for reconciliation.

    /// Cheap capability probe used inside the access gate before
    /// deciding whether to attempt an authorize. Returns
    /// `can_charge=false` with a reason when the user has no payment
    /// method on file, an expired card, etc.
    async fn can_charge(
        &self,
        user_token: &str,
    ) -> Result<BillingCapability, ConnectorError>;

    /// Reserve funds for a single install. Must be idempotent on
    /// `idempotency_key` — replay with the same key returns the same
    /// handle. Returns `PaymentRequired` on decline, `Conflict` on
    /// idempotency-body mismatch.
    async fn authorize_install_charge(
        &self,
        user_token: &str,
        template_id: &Uuid,
        amount_minor: i64,
        currency: &str,
        idempotency_key: &str,
    ) -> Result<AuthorizationHandle, ConnectorError>;

    /// Settle a previously-authorized charge after the install
    /// completes successfully. `auth_token` may be a user token or a
    /// service token depending on caller — deploy-complete uses the
    /// service token. Attaches `deployment_hash` for user_service-side
    /// audit.
    async fn capture_install_charge(
        &self,
        auth_token: &str,
        authorization_id: &str,
        deployment_hash: &str,
    ) -> Result<AuthorizationHandle, ConnectorError>;

    /// Release a previously-authorized charge. Called on install
    /// failure or by the TTL sweeper. `auth_token` semantics identical
    /// to `capture_install_charge`.
    async fn void_install_charge(
        &self,
        auth_token: &str,
        authorization_id: &str,
        reason: &str,
    ) -> Result<(), ConnectorError>;
}
