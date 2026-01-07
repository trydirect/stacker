//! External Service Connectors
//! 
//! This module provides adapters for communicating with external services (User Service, Payment Service, etc.).
//! All external integrations must go through connectors to keep Stacker independent and testable.
//!
//! ## Architecture Pattern
//!
//! 1. Define trait in `{service}.rs` → allows mocking in tests
//! 2. Implement HTTP client in same file
//! 3. Configuration in `config.rs` → enable/disable per environment
//! 4. Inject trait object into routes → routes never depend on HTTP implementation
//!
//! ## Usage in Routes
//!
//! ```ignore
//! // In route handler
//! pub async fn deploy_template(
//!     connector: web::Data<Arc<dyn UserServiceConnector>>,
//! ) -> Result<impl Responder> {
//!     // Routes use trait methods, never care about HTTP details
//!     connector.create_stack_from_template(...).await?;
//! }
//! ```
//!
//! ## Testing
//!
//! ```ignore
//! #[cfg(test)]
//! mod tests {
//!     use super::*;
//!     use connectors::user_service::mock::MockUserServiceConnector;
//!
//!     #[tokio::test]
//!     async fn test_deploy_without_http() {
//!         let connector = Arc::new(MockUserServiceConnector);
//!         // Test route logic without external API calls
//!     }
//! }
//! ```

pub mod config;
pub mod errors;
pub mod admin_service;
pub mod install_service;
pub mod user_service;
pub mod dockerhub_service;

pub use config::{ConnectorConfig, UserServiceConfig, PaymentServiceConfig, EventsConfig};
pub use errors::ConnectorError;
pub use admin_service::{
    parse_jwt_claims,
    validate_jwt_expiration,
    user_from_jwt_claims,
    extract_bearer_token,
};
pub use install_service::{InstallServiceClient, InstallServiceConnector};
pub use user_service::{
    UserServiceConnector, UserServiceClient, StackResponse, UserProfile, UserProduct, ProductInfo,
    UserPlanInfo, PlanDefinition, CategoryInfo,
    DeploymentValidator, DeploymentValidationError,
    MarketplaceWebhookSender, WebhookSenderConfig, MarketplaceWebhookPayload, WebhookResponse,
};

// Re-export init functions for convenient access
pub use user_service::init as init_user_service;
pub use dockerhub_service::init as init_dockerhub;
pub use dockerhub_service::{
    DockerHubClient,
    DockerHubConnector,
    NamespaceSummary,
    RepositorySummary,
    TagSummary,
};
