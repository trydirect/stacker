use serde::{Deserialize, Serialize};

use crate::connectors::errors::ConnectorError;

use super::UserServiceClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionPlan {
    /// Plan name (e.g., "Free", "Basic", "Plus")
    pub name: Option<String>,

    /// Plan code (e.g., "plan-free-periodically", "plan-basic-monthly")
    pub code: Option<String>,

    /// Plan features and limits. User Service may return strings or structured objects.
    pub includes: Option<serde_json::Value>,

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

#[derive(Debug, Clone, Deserialize)]
pub struct UserServiceMePlanResponse {
    pub user: UserServiceMePlanUser,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserServiceMePlanUser {
    pub plan: SubscriptionPlan,
}

impl UserServiceClient {
    /// Get the current `/oauth_server/api/me` response fields required for plan display.
    pub async fn get_me_plan_response(
        &self,
        bearer_token: &str,
    ) -> Result<UserServiceMePlanResponse, ConnectorError> {
        let url = format!("{}/oauth_server/api/me", self.base_url);

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
            .json()
            .await
            .map_err(|e| ConnectorError::InvalidResponse(e.to_string()))
    }

    /// Extract the subscription plan from the current `/oauth_server/api/me` response.
    pub async fn get_subscription_plan(
        &self,
        bearer_token: &str,
    ) -> Result<SubscriptionPlan, ConnectorError> {
        Ok(self.get_me_plan_response(bearer_token).await?.user.plan)
    }
}

pub fn user_service_url_from_sources(
    env_auth_url: Option<String>,
    env_api_url: Option<String>,
    stacker_url: Option<String>,
    config_auth_url: Option<String>,
) -> Option<String> {
    env_auth_url
        .or(env_api_url)
        .or_else(|| stacker_url.and_then(|url| user_service_url_from_stacker_url(&url)))
        .or(config_auth_url)
        .map(|url| normalize_user_service_base_url(&url))
        .filter(|url| !url.is_empty())
}

pub fn user_service_url_from_stacker_url(url: &str) -> Option<String> {
    let normalized = url.trim_end_matches('/').trim_end_matches("/api/v1");
    if let Some(origin) = normalized.strip_suffix("/server/stacker") {
        return Some(format!("{origin}/server/user"));
    }
    if let Some(origin) = normalized.strip_suffix("/stacker") {
        return Some(format!("{origin}/server/user"));
    }
    None
}

pub fn normalize_user_service_base_url(url: &str) -> String {
    let mut normalized = url.trim_end_matches('/').to_string();
    for suffix in ["/oauth_server/api/me", "/auth/login"] {
        if let Some(base) = normalized.strip_suffix(suffix) {
            normalized = base.to_string();
            break;
        }
    }
    normalized
}

pub fn get_subscription_plan_blocking(
    user_service_url: &str,
    bearer_token: &str,
) -> Result<SubscriptionPlan, ConnectorError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| ConnectorError::Internal(format!("Failed to create async runtime: {e}")))?;

    let client = UserServiceClient::new_public(&normalize_user_service_base_url(user_service_url));
    runtime.block_on(client.get_subscription_plan(bearer_token))
}

pub fn subscription_plan_lines(plan: &SubscriptionPlan) -> Vec<String> {
    let mut lines = vec!["Subscription:".to_string()];
    lines.push(format!("  Plan:          {}", plan_label(plan)));
    lines.push(format!("  Status:        {}", plan_status(plan)));
    if let Some(price) = plan_price(plan) {
        lines.push(format!("  Price:         {}", price));
    }
    if let Some(date_end) = plan.date_end.as_deref().filter(|value| !value.is_empty()) {
        lines.push(format!("  Expires at:    {}", date_end));
    }
    if let Some(date_of_purchase) = plan
        .date_of_purchase
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("  Purchased at:  {}", date_of_purchase));
    }
    if let Some(includes) = plan.includes.as_ref().and_then(summarize_includes) {
        lines.push(format!("  Includes:      {}", includes));
    }
    lines
}

fn plan_label(plan: &SubscriptionPlan) -> String {
    match (plan.name.as_deref(), plan.code.as_deref()) {
        (Some(name), Some(code)) => format!("{name} ({code})"),
        (Some(name), None) => name.to_string(),
        (None, Some(code)) => code.to_string(),
        (None, None) => "unknown".to_string(),
    }
}

fn plan_status(plan: &SubscriptionPlan) -> &'static str {
    match plan.active {
        Some(true) => "active",
        Some(false) => "inactive",
        None => "unknown",
    }
}

fn plan_price(plan: &SubscriptionPlan) -> Option<String> {
    let price = plan.price.as_deref()?;
    let mut value = price.to_string();
    if let Some(currency) = plan.currency.as_deref().filter(|value| !value.is_empty()) {
        value.push(' ');
        value.push_str(currency);
    }
    if let Some(period) = plan.period.as_deref().filter(|value| !value.is_empty()) {
        value.push_str(" / ");
        value.push_str(period);
    }
    Some(value)
}

fn summarize_include_entry(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Object(map) => map
            .get("name")
            .or_else(|| map.get("label"))
            .or_else(|| map.get("code"))
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .or_else(|| serde_json::to_string(value).ok()),
        _ => serde_json::to_string(value).ok(),
    }
}

fn summarize_includes(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Array(items) => {
            let labels: Vec<String> = items.iter().filter_map(summarize_include_entry).collect();
            if labels.is_empty() {
                None
            } else {
                Some(labels.join(", "))
            }
        }
        serde_json::Value::Null => None,
        other => summarize_include_entry(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn user_service_url_prefers_env_auth_url() {
        let url = user_service_url_from_sources(
            Some("https://user.example/".to_string()),
            Some("https://api.example".to_string()),
            Some("https://stacker.example".to_string()),
            Some("https://config.example".to_string()),
        );

        assert_eq!(url.as_deref(), Some("https://user.example"));
    }

    #[test]
    fn user_service_url_normalizes_me_endpoint_to_base_url() {
        let url = user_service_url_from_sources(
            Some("https://dev.try.direct/server/user/oauth_server/api/me".to_string()),
            None,
            None,
            None,
        );

        assert_eq!(url.as_deref(), Some("https://dev.try.direct/server/user"));
    }

    #[test]
    fn user_service_url_normalizes_login_endpoint_to_base_url() {
        let url = user_service_url_from_sources(
            Some("https://dev.try.direct/server/user/auth/login".to_string()),
            None,
            None,
            None,
        );

        assert_eq!(url.as_deref(), Some("https://dev.try.direct/server/user"));
    }

    #[test]
    fn user_service_url_derives_from_saved_stacker_url_before_config() {
        let url = user_service_url_from_sources(
            None,
            None,
            Some("https://dev.try.direct/stacker".to_string()),
            Some("https://try.direct/server/user".to_string()),
        );

        assert_eq!(url.as_deref(), Some("https://dev.try.direct/server/user"));
    }

    #[test]
    fn subscription_plan_lines_show_core_plan_fields() {
        let plan = SubscriptionPlan {
            name: Some("Free".to_string()),
            code: Some("plan-free-periodically".to_string()),
            includes: Some(json!([
                { "code": "deploys-20", "name": "20 deploys per month" },
                "Community support"
            ])),
            date_end: None,
            active: Some(true),
            price: Some("0.00".to_string()),
            currency: Some("USD".to_string()),
            period: Some("month".to_string()),
            date_of_purchase: Some("2026-06-15T09:00:00Z".to_string()),
            billing_id: Some("billing-1".to_string()),
        };

        let lines = subscription_plan_lines(&plan);

        assert!(lines
            .iter()
            .any(|line| line.contains("Free (plan-free-periodically)")));
        assert!(lines.iter().any(|line| line.contains("active")));
        assert!(lines.iter().any(|line| line.contains("0.00 USD / month")));
        assert!(lines
            .iter()
            .any(|line| line.contains("20 deploys per month")));
        assert!(lines.iter().any(|line| line.contains("Community support")));
    }
}
