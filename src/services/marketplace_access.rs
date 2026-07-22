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
    /// The template is priced per-install and the user has no viable
    /// payment method on file (declined at the `can_charge` probe, before
    /// any authorize attempt).
    NoPaymentMethod { reason: String },
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
            Self::NoPaymentMethod { reason } => write!(
                f,
                "A valid payment method is required to install this template ({})",
                reason
            ),
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

    let is_per_install = template
        .billing_cycle
        .as_deref()
        .map(|c| c.trim().eq_ignore_ascii_case("per_install"))
        .unwrap_or(false);

    // Per-install templates skip the ownership check entirely — there is no
    // permanent purchase for these. Instead we probe the user's payment
    // capability so `stacker install` fails fast with a clear 402 if the
    // user has no card on file, rather than deep inside the install handler
    // after the actual authorize attempt.
    if is_per_install {
        let capability = user_service
            .can_charge(user_token)
            .await
            .map_err(validation_failed)?;
        if !capability.can_charge {
            return Err(MarketplaceAccessError::NoPaymentMethod {
                reason: capability.reason.unwrap_or_else(|| "unknown".to_string()),
            });
        }
        return Ok(());
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
        AuthorizationHandle, BillingCapability, CategoryInfo, PlanDefinition, ProductInfo,
        StackResponse, UserPlanInfo, UserProduct, UserProfile,
    };
    use crate::connectors::ConnectorError;
    use async_trait::async_trait;
    use std::collections::{HashMap, HashSet, VecDeque};
    use std::sync::Mutex;
    use uuid::Uuid;

    /// Recorded billing-side calls for post-hoc assertion. Used by the
    /// per-install billing tests to check that the access gate called
    /// `can_charge` at the right moment and never invoked authorize/capture/void
    /// (those are the install handler's responsibility).
    #[derive(Debug, Clone, PartialEq)]
    enum CapturedCall {
        CanCharge,
        Authorize {
            template_id: Uuid,
            amount_minor: i64,
            currency: String,
            idempotency_key: String,
        },
        Capture {
            authorization_id: String,
            deployment_hash: String,
        },
        Void {
            authorization_id: String,
            reason: String,
        },
    }

    struct TestUserService {
        plans: HashMap<String, bool>,
        owned_identifiers: HashSet<String>,
        fail_plan_check: bool,
        can_charge_result: Mutex<BillingCapability>,
        authorize_responses: Mutex<VecDeque<Result<AuthorizationHandle, ConnectorError>>>,
        capture_responses: Mutex<VecDeque<Result<AuthorizationHandle, ConnectorError>>>,
        void_responses: Mutex<VecDeque<Result<(), ConnectorError>>>,
        captured_calls: Mutex<Vec<CapturedCall>>,
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
                can_charge_result: Mutex::new(BillingCapability {
                    can_charge: true,
                    reason: None,
                }),
                authorize_responses: Mutex::new(VecDeque::new()),
                capture_responses: Mutex::new(VecDeque::new()),
                void_responses: Mutex::new(VecDeque::new()),
                captured_calls: Mutex::new(Vec::new()),
            }
        }

        /// Make the upstream plan check return a connector error, to exercise
        /// the ValidationFailed error-propagation path.
        fn with_plan_check_failure(mut self) -> Self {
            self.fail_plan_check = true;
            self
        }

        /// Preload the can-charge response used by the per_install access-gate
        /// branch. Default is `{can_charge: true, reason: None}`.
        #[allow(dead_code)]
        fn with_can_charge(self, can_charge: bool, reason: Option<&str>) -> Self {
            *self.can_charge_result.lock().unwrap() = BillingCapability {
                can_charge,
                reason: reason.map(str::to_string),
            };
            self
        }

        /// Snapshot the CapturedCall vector for assertions.
        #[allow(dead_code)]
        fn calls(&self) -> Vec<CapturedCall> {
            self.captured_calls.lock().unwrap().clone()
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

        async fn can_charge(
            &self,
            _user_token: &str,
        ) -> Result<BillingCapability, ConnectorError> {
            self.captured_calls
                .lock()
                .unwrap()
                .push(CapturedCall::CanCharge);
            Ok(self.can_charge_result.lock().unwrap().clone())
        }

        async fn authorize_install_charge(
            &self,
            _user_token: &str,
            template_id: &Uuid,
            amount_minor: i64,
            currency: &str,
            idempotency_key: &str,
        ) -> Result<AuthorizationHandle, ConnectorError> {
            self.captured_calls
                .lock()
                .unwrap()
                .push(CapturedCall::Authorize {
                    template_id: *template_id,
                    amount_minor,
                    currency: currency.to_string(),
                    idempotency_key: idempotency_key.to_string(),
                });
            self.authorize_responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| {
                    Ok(AuthorizationHandle {
                        authorization_id: format!("test-auth-{}", idempotency_key),
                        amount_minor,
                        currency: currency.to_string(),
                        expires_at: Some("2099-01-01T00:00:00Z".to_string()),
                        status: "authorized".to_string(),
                    })
                })
        }

        async fn capture_install_charge(
            &self,
            _auth_token: &str,
            authorization_id: &str,
            deployment_hash: &str,
        ) -> Result<AuthorizationHandle, ConnectorError> {
            self.captured_calls
                .lock()
                .unwrap()
                .push(CapturedCall::Capture {
                    authorization_id: authorization_id.to_string(),
                    deployment_hash: deployment_hash.to_string(),
                });
            self.capture_responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| {
                    Ok(AuthorizationHandle {
                        authorization_id: authorization_id.to_string(),
                        amount_minor: 0,
                        currency: "USD".to_string(),
                        expires_at: None,
                        status: "captured".to_string(),
                    })
                })
        }

        async fn void_install_charge(
            &self,
            _auth_token: &str,
            authorization_id: &str,
            reason: &str,
        ) -> Result<(), ConnectorError> {
            self.captured_calls.lock().unwrap().push(CapturedCall::Void {
                authorization_id: authorization_id.to_string(),
                reason: reason.to_string(),
            });
            self.void_responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(()))
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

    // ── per_install access-gate tests ───────────────────────────────
    //
    // These pin the semantics documented in the plan: a template with
    // billing_cycle="per_install" bypasses ownership entirely, but must
    // still clear the feature-plan gate and expose a valid payment method.

    fn per_install_template() -> models::StackTemplate {
        models::StackTemplate {
            slug: "paid-per-install".to_string(),
            product_id: Some(2001),
            price: Some(9.99),
            billing_cycle: Some("per_install".to_string()),
            required_plan_name: None,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn per_install_template_skips_ownership_and_passes_when_can_charge() {
        // No `owned_identifiers` on purpose — ownership must NOT be consulted
        // for per_install templates.
        let svc = Arc::new(TestUserService::new(&[("professional", true)], &[]));
        let user_service: Arc<dyn UserServiceConnector> = svc.clone();

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &per_install_template())
                .await;

        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        let calls = svc.calls();
        assert!(
            calls.contains(&CapturedCall::CanCharge),
            "gate must consult can_charge for per_install templates: {:?}",
            calls
        );
        assert!(
            !calls.iter().any(|c| matches!(c, CapturedCall::Authorize { .. })),
            "gate must NOT authorize — the install handler does that: {:?}",
            calls
        );
    }

    #[tokio::test]
    async fn per_install_template_rejects_when_no_payment_method() {
        let svc = Arc::new(
            TestUserService::new(&[("professional", true)], &[])
                .with_can_charge(false, Some("no_payment_method")),
        );
        let user_service: Arc<dyn UserServiceConnector> = svc.clone();

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &per_install_template())
                .await;

        assert!(matches!(
            result,
            Err(MarketplaceAccessError::NoPaymentMethod { ref reason })
                if reason == "no_payment_method"
        ));
    }

    #[tokio::test]
    async fn per_install_template_still_requires_feature_plan() {
        // Team/Pro feature-plan gate must precede the per_install branch —
        // a user without the "professional" tier gets rejected before we
        // ever probe payment capability.
        let svc = Arc::new(TestUserService::new(&[("professional", false)], &[]));
        let user_service: Arc<dyn UserServiceConnector> = svc.clone();

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &per_install_template())
                .await;

        assert!(matches!(
            result,
            Err(MarketplaceAccessError::InsufficientFeaturePlan)
        ));
        assert!(
            !svc.calls().contains(&CapturedCall::CanCharge),
            "can_charge must not be probed when the feature-plan gate fails"
        );
    }

    #[tokio::test]
    async fn one_time_template_ownership_check_unchanged() {
        // Regression: `one_time` templates (umami-shape) still use the
        // ownership gate after the split-out per_install branch.
        // umami's row carries required_plan_name="free", which the gate
        // still consults via user_has_plan — every user has the "free"
        // tier in production, so we grant it in the mock too.
        let svc = Arc::new(TestUserService::new(
            &[("professional", true), ("free", true)],
            &[],
        ));
        let user_service: Arc<dyn UserServiceConnector> = svc.clone();
        let template = models::StackTemplate {
            slug: "umami".to_string(),
            product_id: Some(-961115967),
            price: Some(10.0),
            billing_cycle: Some("one_time".to_string()),
            required_plan_name: Some("free".to_string()),
            ..Default::default()
        };

        let result =
            validate_marketplace_template_access(&user_service, &test_user(), &template).await;

        assert!(matches!(
            result,
            Err(MarketplaceAccessError::TemplateNotOwned)
        ));
        assert!(
            !svc.calls().contains(&CapturedCall::CanCharge),
            "can_charge must NOT be probed for one_time templates"
        );
    }
}
