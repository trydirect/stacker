use crate::configuration::PayoutSettings;
use crate::models;
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PayoutOnboardingLink {
    pub provider: String,
    pub account_ref: String,
    pub onboarding_url: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct PayoutOnboardingCompletion {
    pub completed: bool,
    pub payouts_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct PayoutWebhookUpdate {
    pub provider: String,
    pub account_ref: String,
    pub onboarding_completed: bool,
    pub payouts_enabled: bool,
    pub event_type: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PayoutProviderError {
    #[error("Payout provider is not configured: {0}")]
    NotConfigured(String),
    #[error("Payout provider request failed: {0}")]
    Request(String),
    #[error("Payout provider response was invalid: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait PayoutProvider: Send + Sync {
    fn provider_code(&self) -> &'static str;

    async fn create_onboarding_link(
        &self,
        user: &models::User,
        existing_account_ref: Option<&str>,
    ) -> Result<PayoutOnboardingLink, PayoutProviderError>;

    async fn complete_onboarding(
        &self,
        account_ref: &str,
    ) -> Result<PayoutOnboardingCompletion, PayoutProviderError>;

    async fn parse_webhook_update(
        &self,
        payload: &[u8],
        signature: Option<&str>,
    ) -> Result<Option<PayoutWebhookUpdate>, PayoutProviderError>;
}

#[derive(Debug, Default)]
pub struct MockPayoutProvider;

#[async_trait]
impl PayoutProvider for MockPayoutProvider {
    fn provider_code(&self) -> &'static str {
        "mock"
    }

    async fn create_onboarding_link(
        &self,
        _user: &models::User,
        existing_account_ref: Option<&str>,
    ) -> Result<PayoutOnboardingLink, PayoutProviderError> {
        let account_ref = existing_account_ref
            .map(str::to_string)
            .unwrap_or_else(|| format!("acct_mock_{}", uuid::Uuid::new_v4().simple()));

        Ok(PayoutOnboardingLink {
            provider: self.provider_code().to_string(),
            onboarding_url: Some(format!(
                "https://mock.payouts.local/onboarding/{account_ref}"
            )),
            account_ref,
            expires_at: None,
        })
    }

    async fn complete_onboarding(
        &self,
        _account_ref: &str,
    ) -> Result<PayoutOnboardingCompletion, PayoutProviderError> {
        Ok(PayoutOnboardingCompletion {
            completed: true,
            payouts_enabled: false,
        })
    }

    async fn parse_webhook_update(
        &self,
        _payload: &[u8],
        _signature: Option<&str>,
    ) -> Result<Option<PayoutWebhookUpdate>, PayoutProviderError> {
        Ok(None)
    }
}

#[derive(Clone)]
pub struct StripeConnectPayoutProvider {
    http_client: reqwest::Client,
    secret_key: String,
    webhook_secret: String,
    api_base_url: String,
    return_url: String,
    refresh_url: String,
}

impl StripeConnectPayoutProvider {
    pub fn try_new(settings: &PayoutSettings) -> Result<Self, PayoutProviderError> {
        let secret_key = settings.stripe_secret_key.trim().to_string();
        if secret_key.is_empty() {
            return Err(PayoutProviderError::NotConfigured(
                "stripe_secret_key is required when payouts.provider=stripe_connect".to_string(),
            ));
        }
        let webhook_secret = settings.stripe_webhook_secret.trim().to_string();
        if webhook_secret.is_empty() {
            return Err(PayoutProviderError::NotConfigured(
                "stripe_webhook_secret is required when payouts.provider=stripe_connect"
                    .to_string(),
            ));
        }

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(settings.timeout_secs))
            .build()
            .map_err(|err| PayoutProviderError::Request(err.to_string()))?;

        Ok(Self {
            http_client,
            secret_key,
            webhook_secret,
            api_base_url: settings
                .stripe_api_base_url
                .trim_end_matches('/')
                .to_string(),
            return_url: settings.onboarding_return_url.clone(),
            refresh_url: settings.onboarding_refresh_url.clone(),
        })
    }

    async fn create_account(&self, user: &models::User) -> Result<String, PayoutProviderError> {
        #[derive(Deserialize)]
        struct StripeAccountResponse {
            id: String,
        }

        let response = self
            .http_client
            .post(format!("{}/v1/accounts", self.api_base_url))
            .bearer_auth(&self.secret_key)
            .form(&[
                ("type", "express"),
                ("email", user.email.as_str()),
                ("metadata[stacker_user_id]", user.id.as_str()),
            ])
            .send()
            .await
            .map_err(|err| PayoutProviderError::Request(err.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(PayoutProviderError::Request(format!(
                "Stripe account create failed with status {status}: {body}"
            )));
        }

        response
            .json::<StripeAccountResponse>()
            .await
            .map(|account| account.id)
            .map_err(|err| PayoutProviderError::InvalidResponse(err.to_string()))
    }
}

#[async_trait]
impl PayoutProvider for StripeConnectPayoutProvider {
    fn provider_code(&self) -> &'static str {
        "stripe_connect"
    }

    async fn create_onboarding_link(
        &self,
        user: &models::User,
        existing_account_ref: Option<&str>,
    ) -> Result<PayoutOnboardingLink, PayoutProviderError> {
        #[derive(Deserialize)]
        struct StripeAccountLinkResponse {
            url: String,
            expires_at: i64,
        }

        let account_ref = match existing_account_ref {
            Some(account_ref) if !account_ref.trim().is_empty() => account_ref.to_string(),
            _ => self.create_account(user).await?,
        };

        let response = self
            .http_client
            .post(format!("{}/v1/account_links", self.api_base_url))
            .bearer_auth(&self.secret_key)
            .form(&[
                ("account", account_ref.as_str()),
                ("refresh_url", self.refresh_url.as_str()),
                ("return_url", self.return_url.as_str()),
                ("type", "account_onboarding"),
            ])
            .send()
            .await
            .map_err(|err| PayoutProviderError::Request(err.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(PayoutProviderError::Request(format!(
                "Stripe account link create failed with status {status}: {body}"
            )));
        }

        let link = response
            .json::<StripeAccountLinkResponse>()
            .await
            .map_err(|err| PayoutProviderError::InvalidResponse(err.to_string()))?;

        Ok(PayoutOnboardingLink {
            provider: self.provider_code().to_string(),
            account_ref,
            onboarding_url: Some(link.url),
            expires_at: Some(link.expires_at),
        })
    }

    async fn complete_onboarding(
        &self,
        account_ref: &str,
    ) -> Result<PayoutOnboardingCompletion, PayoutProviderError> {
        #[derive(Deserialize)]
        struct StripeAccountStatusResponse {
            details_submitted: bool,
            payouts_enabled: bool,
        }

        let response = self
            .http_client
            .get(format!("{}/v1/accounts/{}", self.api_base_url, account_ref))
            .bearer_auth(&self.secret_key)
            .send()
            .await
            .map_err(|err| PayoutProviderError::Request(err.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(PayoutProviderError::Request(format!(
                "Stripe account retrieve failed with status {status}: {body}"
            )));
        }

        let account = response
            .json::<StripeAccountStatusResponse>()
            .await
            .map_err(|err| PayoutProviderError::InvalidResponse(err.to_string()))?;

        Ok(PayoutOnboardingCompletion {
            completed: account.details_submitted,
            payouts_enabled: account.payouts_enabled,
        })
    }

    async fn parse_webhook_update(
        &self,
        payload: &[u8],
        signature: Option<&str>,
    ) -> Result<Option<PayoutWebhookUpdate>, PayoutProviderError> {
        verify_stripe_signature(payload, signature, &self.webhook_secret)?;

        let event = serde_json::from_slice::<Value>(payload)
            .map_err(|err| PayoutProviderError::InvalidResponse(err.to_string()))?;
        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        if event_type != "account.updated" {
            return Ok(None);
        }

        let account = event
            .get("data")
            .and_then(|data| data.get("object"))
            .ok_or_else(|| {
                PayoutProviderError::InvalidResponse(
                    "Stripe account.updated event missing data.object".to_string(),
                )
            })?;
        let account_ref = account
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PayoutProviderError::InvalidResponse(
                    "Stripe account.updated event missing account id".to_string(),
                )
            })?
            .to_string();
        let onboarding_completed = account
            .get("details_submitted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let payouts_enabled = account
            .get("payouts_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        Ok(Some(PayoutWebhookUpdate {
            provider: self.provider_code().to_string(),
            account_ref,
            onboarding_completed,
            payouts_enabled,
            event_type,
        }))
    }
}

fn verify_stripe_signature(
    payload: &[u8],
    signature_header: Option<&str>,
    webhook_secret: &str,
) -> Result<(), PayoutProviderError> {
    let signature_header = signature_header.ok_or_else(|| {
        PayoutProviderError::Request("Missing Stripe-Signature header".to_string())
    })?;
    let timestamp = stripe_signature_part(signature_header, "t").ok_or_else(|| {
        PayoutProviderError::Request("Stripe-Signature header missing timestamp".to_string())
    })?;
    let timestamp_i64 = timestamp.parse::<i64>().map_err(|_| {
        PayoutProviderError::Request("Stripe-Signature timestamp is invalid".to_string())
    })?;
    let now = chrono::Utc::now().timestamp();
    if (now - timestamp_i64).abs() > 300 {
        return Err(PayoutProviderError::Request(
            "Stripe webhook signature timestamp is outside tolerance".to_string(),
        ));
    }

    let expected_signature = stripe_signature_part(signature_header, "v1").ok_or_else(|| {
        PayoutProviderError::Request("Stripe-Signature header missing v1 signature".to_string())
    })?;
    let expected_signature = hex_decode(expected_signature)?;

    let mut mac = Hmac::<Sha256>::new_from_slice(webhook_secret.as_bytes())
        .map_err(|err| PayoutProviderError::Request(err.to_string()))?;
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(payload);
    mac.verify_slice(&expected_signature)
        .map_err(|_| PayoutProviderError::Request("Invalid Stripe webhook signature".to_string()))
}

fn stripe_signature_part<'a>(header: &'a str, key: &str) -> Option<&'a str> {
    header.split(',').find_map(|part| {
        let (part_key, part_value) = part.split_once('=')?;
        (part_key == key).then_some(part_value)
    })
}

fn hex_decode(value: &str) -> Result<Vec<u8>, PayoutProviderError> {
    if value.len() % 2 != 0 {
        return Err(PayoutProviderError::Request(
            "Stripe webhook signature hex is invalid".to_string(),
        ));
    }

    let mut bytes = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().chunks_exact(2) {
        let high = hex_value(chunk[0])?;
        let low = hex_value(chunk[1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_value(byte: u8) -> Result<u8, PayoutProviderError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(PayoutProviderError::Request(
            "Stripe webhook signature hex is invalid".to_string(),
        )),
    }
}

pub fn init_payout_provider(
    settings: &PayoutSettings,
) -> Result<std::sync::Arc<dyn PayoutProvider>, PayoutProviderError> {
    match settings.provider.as_str() {
        "stripe_connect" | "stripe" => Ok(std::sync::Arc::new(
            StripeConnectPayoutProvider::try_new(settings)?,
        )),
        "mock" | "" => Ok(std::sync::Arc::new(MockPayoutProvider)),
        other => Err(PayoutProviderError::NotConfigured(format!(
            "Unknown payout provider '{other}'. Expected 'mock' or 'stripe_connect'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::PayoutSettings;

    fn stripe_settings() -> PayoutSettings {
        PayoutSettings {
            provider: "stripe_connect".to_string(),
            stripe_secret_key: "sk_test_123".to_string(),
            stripe_webhook_secret: "whsec_test".to_string(),
            stripe_api_base_url: "https://api.stripe.com".to_string(),
            onboarding_return_url: "https://example.com/return".to_string(),
            onboarding_refresh_url: "https://example.com/refresh".to_string(),
            timeout_secs: 5,
        }
    }

    fn test_hex_encode(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        out
    }

    fn sign_payload(payload: &[u8], timestamp: i64, secret: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload);
        format!(
            "t={},v1={}",
            timestamp,
            test_hex_encode(&mac.finalize().into_bytes())
        )
    }

    #[test]
    fn stripe_provider_requires_webhook_secret() {
        let mut settings = stripe_settings();
        settings.stripe_webhook_secret.clear();

        let err = match StripeConnectPayoutProvider::try_new(&settings) {
            Ok(_) => panic!("stripe provider should require webhook secret"),
            Err(err) => err,
        };
        assert!(err
            .to_string()
            .contains("stripe_webhook_secret is required"));
    }

    #[tokio::test]
    async fn stripe_webhook_rejects_stale_signature() {
        let provider = StripeConnectPayoutProvider::try_new(&stripe_settings()).unwrap();
        let payload = br#"{"type":"account.updated","data":{"object":{"id":"acct_1","details_submitted":true,"payouts_enabled":true}}}"#;
        let stale_timestamp = chrono::Utc::now().timestamp() - 1_000;
        let signature = sign_payload(payload, stale_timestamp, "whsec_test");

        let err = provider
            .parse_webhook_update(payload, Some(&signature))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("outside tolerance"));
    }

    #[tokio::test]
    async fn stripe_webhook_parses_valid_account_update() {
        let provider = StripeConnectPayoutProvider::try_new(&stripe_settings()).unwrap();
        let payload = br#"{"type":"account.updated","data":{"object":{"id":"acct_1","details_submitted":true,"payouts_enabled":true}}}"#;
        let signature = sign_payload(payload, chrono::Utc::now().timestamp(), "whsec_test");

        let update = provider
            .parse_webhook_update(payload, Some(&signature))
            .await
            .unwrap()
            .expect("account.updated should produce update");

        assert_eq!("stripe_connect", update.provider);
        assert_eq!("acct_1", update.account_ref);
        assert!(update.onboarding_completed);
        assert!(update.payouts_enabled);
    }
}
