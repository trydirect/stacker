use std::sync::Arc;

use crate::connectors::{errors::ConnectorError, user_service::UserServiceConnector};
use crate::models;

const MARKETPLACE_INSTALL_MIN_PLAN: &str = "professional";

#[derive(Debug)]
pub enum MarketplaceAccessError {
    MissingUserToken,
    InsufficientFeaturePlan,
    InsufficientTemplatePlan { required_plan: String },
    TemplateNotOwned,
    ValidationFailed(String),
}

impl std::fmt::Display for MarketplaceAccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingUserToken => {
                write!(f, "User token is required to validate marketplace access")
            }
            Self::InsufficientFeaturePlan => write!(
                f,
                "Marketplace installs are available on Pro and Team plans"
            ),
            Self::InsufficientTemplatePlan { required_plan } => write!(
                f,
                "This template requires a '{}' subscription",
                required_plan
            ),
            Self::TemplateNotOwned => {
                write!(f, "You must purchase this template before installing it")
            }
            Self::ValidationFailed(reason) => {
                write!(f, "Failed to validate marketplace access: {}", reason)
            }
        }
    }
}

impl std::error::Error for MarketplaceAccessError {}

fn validation_failed(err: ConnectorError) -> MarketplaceAccessError {
    MarketplaceAccessError::ValidationFailed(err.to_string())
}

async fn user_owns_template_by_any_identifier(
    user_service: &Arc<dyn UserServiceConnector>,
    user_token: &str,
    template: &models::StackTemplate,
) -> Result<bool, MarketplaceAccessError> {
    let identifiers = [
        template.product_id.map(|id| id.to_string()),
        Some(template.id.to_string()),
        Some(template.slug.clone()),
    ];

    for identifier in identifiers.into_iter().flatten() {
        if user_service
            .user_owns_template(user_token, &identifier)
            .await
            .map_err(validation_failed)?
        {
            return Ok(true);
        }
    }

    Ok(false)
}

pub async fn validate_marketplace_template_access(
    user_service: &Arc<dyn UserServiceConnector>,
    user: &models::User,
    template: &models::StackTemplate,
) -> Result<(), MarketplaceAccessError> {
    let user_token = user
        .access_token
        .as_deref()
        .ok_or(MarketplaceAccessError::MissingUserToken)?;

    let has_feature_plan = user_service
        .user_has_plan(&user.id, MARKETPLACE_INSTALL_MIN_PLAN, Some(user_token))
        .await
        .map_err(validation_failed)?;
    if !has_feature_plan {
        return Err(MarketplaceAccessError::InsufficientFeaturePlan);
    }

    if let Some(required_plan) = template
        .required_plan_name
        .as_deref()
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
    {
        let has_template_plan = user_service
            .user_has_plan(&user.id, required_plan, Some(user_token))
            .await
            .map_err(validation_failed)?;
        if !has_template_plan {
            return Err(MarketplaceAccessError::InsufficientTemplatePlan {
                required_plan: required_plan.to_string(),
            });
        }
    }

    let no_price = template.price.map(|p| p <= 0.0).unwrap_or(true);
    let no_plan = template
        .required_plan_name
        .as_deref()
        .map(|p| p.trim().is_empty() || p.trim().eq_ignore_ascii_case("free"))
        .unwrap_or(true);
    let is_free = no_price && no_plan;

    if !is_free
        && template.product_id.is_some()
        && !user_owns_template_by_any_identifier(user_service, user_token, template).await?
    {
        return Err(MarketplaceAccessError::TemplateNotOwned);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::user_service::{
        CategoryInfo, PlanDefinition, ProductInfo, StackResponse, UserPlanInfo, UserProduct,
        UserProfile,
    };
    use crate::connectors::ConnectorError;
    use async_trait::async_trait;
    use std::collections::{HashMap, HashSet};
    use uuid::Uuid;

    struct TestUserService {
        plans: HashMap<String, bool>,
        owned_identifiers: HashSet<String>,
        fail_plan_check: bool,
    }

    impl TestUserService {
        fn new(plans: &[(&str, bool)], owned_identifiers: &[&str]) -> Self {
            Self {
                plans: plans
                    .iter()
                    .map(|(plan, allowed)| ((*plan).to_string(), *allowed))
                    .collect(),
                owned_identifiers: owned_identifiers
                    .iter()
                    .map(|identifier| (*identifier).to_string())
                    .collect(),
                fail_plan_check: false,
            }
        }

        /// Make the upstream plan check return a connector error, to exercise
        /// the ValidationFailed error-propagation path.
        fn with_plan_check_failure(mut self) -> Self {
            self.fail_plan_check = true;
            self
        }
    }

    #[async_trait]
    impl UserServiceConnector for TestUserService {
        async fn create_stack_from_template(
            &self,
            _marketplace_template_id: &Uuid,
            _user_id: &str,
            _template_version: &str,
            _name: &str,
            _stack_definition: serde_json::Value,
        ) -> Result<StackResponse, ConnectorError> {
            unimplemented!()
        }

        async fn get_stack(
            &self,
            _stack_id: i32,
            _user_id: &str,
        ) -> Result<StackResponse, ConnectorError> {
            unimplemented!()
        }

        async fn list_stacks(&self, _user_id: &str) -> Result<Vec<StackResponse>, ConnectorError> {
            unimplemented!()
        }

        async fn user_has_plan(
            &self,
            _user_id: &str,
            required_plan_name: &str,
            _user_token: Option<&str>,
        ) -> Result<bool, ConnectorError> {
            if self.fail_plan_check {
                return Err(ConnectorError::ServiceUnavailable(
                    "user service unreachable".to_string(),
                ));
            }
            Ok(self.plans.get(required_plan_name).copied().unwrap_or(false))
        }

        async fn get_user_plan(&self, _user_id: &str) -> Result<UserPlanInfo, ConnectorError> {
            unimplemented!()
        }

        async fn list_available_plans(&self) -> Result<Vec<PlanDefinition>, ConnectorError> {
            unimplemented!()
        }

        async fn get_user_profile(&self, _user_token: &str) -> Result<UserProfile, ConnectorError> {
            Ok(UserProfile {
                id: "test-user-id".to_string(),
                email: "user@example.com".to_string(),
                plan: None,
                products: vec![UserProduct {
                    id: Some("product-id".to_string()),
                    name: "Template".to_string(),
                    code: "paid-template".to_string(),
                    product_type: "template".to_string(),
                    external_id: Some(100),
                    owned_since: None,
                }],
            })
        }

        async fn get_template_product(
            &self,
            _stack_template_id: i32,
        ) -> Result<Option<ProductInfo>, ConnectorError> {
            unimplemented!()
        }

        async fn user_owns_template(
            &self,
            _user_token: &str,
            stack_template_id: &str,
        ) -> Result<bool, ConnectorError> {
            Ok(self.owned_identifiers.contains(stack_template_id))
        }

        async fn get_categories(&self) -> Result<Vec<CategoryInfo>, ConnectorError> {
            unimplemented!()
        }

        async fn search_marketplace_templates(
            &self,
            _user_token: &str,
            _query: Option<&str>,
            _category: Option<&str>,
            _is_marketplace: Option<bool>,
            _page: Option<u32>,
            _max_results: Option<u32>,
        ) -> Result<Vec<serde_json::Value>, ConnectorError> {
            unimplemented!()
        }
    }

    fn test_user() -> models::User {
        models::User {
            id: "user-1".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            email: "user@example.com".to_string(),
            role: "user".to_string(),
            email_confirmed: true,
            mfa_verified: false,
            access_token: Some("token".to_string()),
        }
    }

    fn test_template() -> models::StackTemplate {
        models::StackTemplate {
            slug: "paid-template".to_string(),
            product_id: Some(100),
            required_plan_name: Some("enterprise".to_string()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn validates_feature_plan_template_plan_and_ownership() {
        let user_service: Arc<dyn UserServiceConnector> = Arc::new(TestUserService::new(
            &[("professional", true), ("enterprise", true)],
            &["100"],
        ));

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &test_template())
                .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn rejects_users_below_marketplace_install_plan() {
        let user_service: Arc<dyn UserServiceConnector> =
            Arc::new(TestUserService::new(&[("professional", false)], &["100"]));

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &test_template())
                .await;

        assert!(matches!(
            result,
            Err(MarketplaceAccessError::InsufficientFeaturePlan)
        ));
    }

    #[tokio::test]
    async fn rejects_unowned_paid_templates() {
        let user_service: Arc<dyn UserServiceConnector> = Arc::new(TestUserService::new(
            &[("professional", true), ("enterprise", true)],
            &[],
        ));

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &test_template())
                .await;

        assert!(matches!(
            result,
            Err(MarketplaceAccessError::TemplateNotOwned)
        ));
    }

    #[tokio::test]
    async fn allows_free_templates_without_ownership() {
        let user_service: Arc<dyn UserServiceConnector> =
            Arc::new(TestUserService::new(&[("professional", true)], &[]));

        let template = models::StackTemplate {
            slug: "free-template".to_string(),
            product_id: Some(999),
            price: None,
            required_plan_name: None,
            ..Default::default()
        };

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &template).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn allows_zero_price_templates_without_ownership() {
        let user_service: Arc<dyn UserServiceConnector> =
            Arc::new(TestUserService::new(&[("professional", true)], &[]));

        let template = models::StackTemplate {
            slug: "zero-price-template".to_string(),
            product_id: Some(888),
            price: Some(0.0),
            required_plan_name: None,
            ..Default::default()
        };

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &template).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn rejects_unowned_template_with_plan_requirement() {
        let user_service: Arc<dyn UserServiceConnector> = Arc::new(TestUserService::new(
            &[("professional", true), ("enterprise", true)],
            &[],
        ));

        let template = models::StackTemplate {
            slug: "plan-only-template".to_string(),
            product_id: Some(777),
            price: None,
            required_plan_name: Some("enterprise".to_string()),
            ..Default::default()
        };

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &template).await;

        assert!(matches!(
            result,
            Err(MarketplaceAccessError::TemplateNotOwned)
        ));
    }

    #[tokio::test]
    async fn rejects_when_user_token_missing() {
        let user_service: Arc<dyn UserServiceConnector> =
            Arc::new(TestUserService::new(&[("professional", true)], &["100"]));

        let mut user = test_user();
        user.access_token = None;

        let result =
            validate_marketplace_template_access(&user_service, &user, &test_template()).await;

        assert!(matches!(
            result,
            Err(MarketplaceAccessError::MissingUserToken)
        ));
    }

    #[tokio::test]
    async fn rejects_when_template_requires_higher_plan_than_feature_plan() {
        // User meets the marketplace-install minimum (professional) but not the
        // template's required plan (enterprise).
        let user_service: Arc<dyn UserServiceConnector> = Arc::new(TestUserService::new(
            &[("professional", true), ("enterprise", false)],
            &["100"],
        ));

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &test_template())
                .await;

        assert!(matches!(
            result,
            Err(MarketplaceAccessError::InsufficientTemplatePlan { required_plan })
                if required_plan == "enterprise"
        ));
    }

    #[tokio::test]
    async fn allows_when_user_owns_template_by_uuid() {
        let template_id = Uuid::new_v4();
        let template = models::StackTemplate {
            id: template_id,
            slug: "paid-template".to_string(),
            product_id: Some(100),
            price: Some(5.0),
            required_plan_name: None,
            ..Default::default()
        };

        // Ownership is recorded under the UUID, not the product_id — the resolver
        // must fall through product_id -> UUID.
        let user_service: Arc<dyn UserServiceConnector> = Arc::new(TestUserService::new(
            &[("professional", true)],
            &[template_id.to_string().as_str()],
        ));

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &template).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn allows_when_user_owns_template_by_slug() {
        let template = models::StackTemplate {
            id: Uuid::new_v4(),
            slug: "paid-template".to_string(),
            product_id: Some(100),
            price: Some(5.0),
            required_plan_name: None,
            ..Default::default()
        };

        // Ownership is recorded only under the slug — the resolver must fall
        // through product_id -> UUID -> slug.
        let user_service: Arc<dyn UserServiceConnector> = Arc::new(TestUserService::new(
            &[("professional", true)],
            &["paid-template"],
        ));

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &template).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn propagates_connector_error_as_validation_failed() {
        let user_service: Arc<dyn UserServiceConnector> = Arc::new(
            TestUserService::new(&[("professional", true)], &["100"]).with_plan_check_failure(),
        );

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &test_template())
                .await;

        assert!(matches!(
            result,
            Err(MarketplaceAccessError::ValidationFailed(_))
        ));
    }
}
