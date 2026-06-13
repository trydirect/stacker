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

    if template.product_id.is_some()
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
            }
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
}
