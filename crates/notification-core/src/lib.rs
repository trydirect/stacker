//! Pure notification channel logic for TryDirect Stacker (WS-N).
//!
//! Contracts: config/shared-fixtures/api-contracts/notifications-channels.json
//! and notifications-events.json. Plan: config/PLAN-notifications.md.
//!
//! This crate owns event shaping, per-channel payloads, secret masking,
//! error redaction and delivery with retry/backoff. It does no database,
//! Vault or auth work — the Stacker service layer (`services/notification.rs`)
//! persists dispatch records and fetches secrets.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::OnceLock;
use std::time::Duration;

pub const MAX_DELIVERY_ATTEMPTS: i32 = 3;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Event types a channel can subscribe to (see the events contract).
pub const NOTIFICATION_EVENT_TYPES: [&str; 8] = [
    "deploy.succeeded",
    "deploy.failed",
    "backup.succeeded",
    "backup.failed",
    "preview.ready",
    "preview.failed",
    "container.down",
    "channel.test",
];

pub const CHANNEL_TYPES: [&str; 5] = ["slack", "discord", "telegram", "email", "webhook"];

pub fn is_valid_channel_type(channel_type: &str) -> bool {
    CHANNEL_TYPES.contains(&channel_type)
}

/// Map a deployment status transition to an event type. Edge-triggered:
/// same-status updates and non-terminal states return `None`, so repeated
/// progress messages produce exactly one notification.
pub fn deploy_event_type(previous_status: &str, new_status: &str) -> Option<&'static str> {
    if previous_status == new_status {
        return None;
    }
    match new_status {
        "completed" | "complete" => Some("deploy.succeeded"),
        "failed" | "error" => Some("deploy.failed"),
        _ => None,
    }
}

/// Build the canonical event payload (see the events contract).
pub fn build_event(
    event_type: &str,
    project_id: Option<i64>,
    subject_kind: &str,
    subject_id: &str,
    subject_name: &str,
    message: &str,
) -> serde_json::Value {
    serde_json::json!({
        "eventType": event_type,
        "occurredAt": chrono::Utc::now().to_rfc3339(),
        "projectId": project_id,
        "subject": { "kind": subject_kind, "id": subject_id, "name": subject_name },
        "message": message,
        "dedupeKey": format!("{}:{}:{}", event_type, subject_id, message),
    })
}

/// Masked target for display. Never returns the full secret.
pub fn mask_target(secret: &serde_json::Value, channel_type: &str) -> String {
    match channel_type {
        "telegram" => {
            let chat = secret.get("chatId").and_then(|v| v.as_str()).unwrap_or("?");
            format!("telegram chat {}", chat)
        }
        "email" => {
            let to = secret.get("to").and_then(|v| v.as_str()).unwrap_or("?");
            match to.split_once('@') {
                Some((local, domain)) => {
                    format!("{}***@{}", local.chars().next().unwrap_or('*'), domain)
                }
                None => "***".to_string(),
            }
        }
        _ => {
            let url = secret.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            match url.split_once("//") {
                Some((scheme, rest)) => match rest.split_once('/') {
                    Some((host, _)) => format!("{}//{}/***", scheme, host),
                    None => format!("{}//{}", scheme, rest),
                },
                None => "***".to_string(),
            }
        }
    }
}

/// Remove secret material from an error string before storing/returning it.
/// Walks nested JSON values as well.
pub fn redact_error(error: &str, secret: &serde_json::Value) -> String {
    fn collect_strings(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
            serde_json::Value::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
            _ => {}
        }
    }

    let mut secrets = Vec::new();
    collect_strings(secret, &mut secrets);

    let mut redacted = error.to_string();
    for s in secrets {
        if s.len() > 3 {
            redacted = redacted.replace(&s, "***");
        }
    }
    redacted.chars().take(500).collect()
}

/// Basic secret-shape validation: url-based types must carry an http(s) URL.
/// SSRF hardening (private-range blocking) is tracked in PLAN-notifications.md.
pub fn validate_secret(channel_type: &str, secret: &serde_json::Value) -> Result<(), String> {
    let get = |key: &str| secret.get(key).and_then(|v| v.as_str()).unwrap_or_default();
    match channel_type {
        "slack" | "discord" | "webhook" => {
            let url = get("url");
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err("secret.url must be an http(s) URL".to_string());
            }
        }
        "telegram" => {
            if get("botToken").is_empty() || get("chatId").is_empty() {
                return Err("telegram secret must contain botToken and chatId".to_string());
            }
        }
        "email" => {
            if !get("to").contains('@') {
                return Err("email secret must contain a 'to' address".to_string());
            }
        }
        other => return Err(format!("unsupported channel type: {}", other)),
    }
    Ok(())
}

/// A single delivery failure, classified for retry.
#[derive(Debug)]
pub struct DeliveryFailure {
    pub retryable: bool,
    pub message: String,
}

/// Shared HTTP client with a request timeout: one pool, no unbounded hangs
/// on a wedged webhook endpoint.
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

/// Send the event once. The `email` channel is delivered through the notify
/// service by the Stacker service layer and fails fast here.
pub async fn send_once(
    channel_type: &str,
    secret: &serde_json::Value,
    event: &serde_json::Value,
) -> Result<(), DeliveryFailure> {
    let message = event
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("Stacker notification")
        .to_string();

    let fatal = |message: String| DeliveryFailure {
        retryable: false,
        message,
    };

    let request = match channel_type {
        "slack" => {
            let url = secret
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| fatal("slack secret must contain url".to_string()))?;
            http_client()
                .post(url)
                .json(&serde_json::json!({ "text": message }))
        }
        "discord" => {
            let url = secret
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| fatal("discord secret must contain url".to_string()))?;
            http_client()
                .post(url)
                .json(&serde_json::json!({ "content": message }))
        }
        "telegram" => {
            let token = secret
                .get("botToken")
                .and_then(|v| v.as_str())
                .ok_or_else(|| fatal("telegram secret must contain botToken".to_string()))?;
            let chat_id = secret
                .get("chatId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| fatal("telegram secret must contain chatId".to_string()))?;
            http_client()
                .post(format!("https://api.telegram.org/bot{}/sendMessage", token))
                .json(&serde_json::json!({ "chat_id": chat_id, "text": message }))
        }
        "webhook" => {
            let url = secret
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| fatal("webhook secret must contain url".to_string()))?;
            let mut request = http_client().post(url).json(event);
            if let Some(key) = secret.get("hmacKey").and_then(|v| v.as_str()) {
                let body = serde_json::to_string(event).unwrap_or_default();
                let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes())
                    .map_err(|e| fatal(format!("invalid hmac key: {}", e)))?;
                mac.update(body.as_bytes());
                let signature: String = mac
                    .finalize()
                    .into_bytes()
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect();
                request = request.header("X-Stacker-Signature", signature);
            }
            request
        }
        "email" => {
            return Err(fatal(
                "email delivery goes through the notify service (N4)".to_string(),
            ))
        }
        other => return Err(fatal(format!("unsupported channel type: {}", other))),
    };

    let response = request.send().await.map_err(|e| DeliveryFailure {
        retryable: true,
        message: format!("request failed: {}", e),
    })?;

    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let retryable = status.as_u16() == 429 || status.is_server_error();
    let body = response.text().await.unwrap_or_default();
    Err(DeliveryFailure {
        retryable,
        message: format!("HTTP {}: {}", status, body),
    })
}

/// Deliver with retry/backoff on 429/5xx (at-least-once). Returns the number
/// of attempts on success; on failure the error string is already redacted
/// with the secret material.
pub async fn deliver(
    channel_type: &str,
    secret: &serde_json::Value,
    event: &serde_json::Value,
) -> Result<i32, (i32, String)> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match send_once(channel_type, secret, event).await {
            Ok(()) => return Ok(attempts),
            Err(failure) => {
                let message = redact_error(&failure.message, secret);
                if !failure.retryable || attempts >= MAX_DELIVERY_ATTEMPTS {
                    return Err((attempts, message));
                }
                tokio::time::sleep(Duration::from_millis(500 * attempts as u64)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_channel_type() {
        assert!(is_valid_channel_type("slack"));
        assert!(is_valid_channel_type("webhook"));
        assert!(!is_valid_channel_type("sms"));
        assert!(!is_valid_channel_type(""));
    }

    #[test]
    fn test_deploy_event_type_is_edge_triggered() {
        assert_eq!(deploy_event_type("in_progress", "completed"), Some("deploy.succeeded"));
        assert_eq!(deploy_event_type("in_progress", "complete"), Some("deploy.succeeded"));
        assert_eq!(deploy_event_type("in_progress", "failed"), Some("deploy.failed"));
        assert_eq!(deploy_event_type("in_progress", "error"), Some("deploy.failed"));
        // No repeated notification for the same terminal status.
        assert_eq!(deploy_event_type("failed", "failed"), None);
        assert_eq!(deploy_event_type("completed", "completed"), None);
        // Non-terminal states do not notify.
        assert_eq!(deploy_event_type("in_progress", "paused"), None);
        assert_eq!(deploy_event_type("paused", "in_progress"), None);
    }

    #[test]
    fn test_mask_target_url_keeps_only_host() {
        let secret = serde_json::json!({ "url": "https://hooks.slack.com/services/T0/B0/XXX" });
        let hint = mask_target(&secret, "slack");
        assert_eq!(hint, "https://hooks.slack.com/***");
        assert!(!hint.contains("XXX"));
    }

    #[test]
    fn test_mask_target_telegram_shows_chat_only() {
        let secret = serde_json::json!({ "botToken": "BOT-SECRET", "chatId": "42" });
        let hint = mask_target(&secret, "telegram");
        assert_eq!(hint, "telegram chat 42");
        assert!(!hint.contains("BOT-SECRET"));
    }

    #[test]
    fn test_mask_target_email_masks_local_part() {
        let secret = serde_json::json!({ "to": "alice@example.com" });
        let hint = mask_target(&secret, "email");
        assert_eq!(hint, "a***@example.com");
        assert!(!hint.contains("alice"));
    }

    #[test]
    fn test_redact_error_strips_secret_material() {
        let secret = serde_json::json!({ "url": "https://hooks.slack.com/services/T0/B0/XXX" });
        let error = "POST https://hooks.slack.com/services/T0/B0/XXX returned 403";
        let redacted = redact_error(error, &secret);
        assert!(!redacted.contains("XXX"));
        assert!(redacted.contains("***"));
    }

    #[test]
    fn test_redact_error_walks_nested_secrets() {
        let secret = serde_json::json!({ "nested": { "token": "NESTED-SECRET-VALUE" } });
        let redacted = redact_error("failure with NESTED-SECRET-VALUE inside", &secret);
        assert!(!redacted.contains("NESTED-SECRET-VALUE"));
    }

    #[test]
    fn test_redact_error_truncates_long_output() {
        let secret = serde_json::json!({ "url": "https://example.com/hook" });
        let error = "x".repeat(2000);
        let redacted = redact_error(&error, &secret);
        assert!(redacted.chars().count() <= 500);
    }

    #[test]
    fn test_validate_secret() {
        assert!(validate_secret("slack", &serde_json::json!({ "url": "https://x.example/h" })).is_ok());
        assert!(validate_secret("slack", &serde_json::json!({ "url": "ftp://x" })).is_err());
        assert!(validate_secret("telegram", &serde_json::json!({ "botToken": "t", "chatId": "1" })).is_ok());
        assert!(validate_secret("telegram", &serde_json::json!({ "botToken": "t" })).is_err());
        assert!(validate_secret("email", &serde_json::json!({ "to": "a@b.c" })).is_ok());
        assert!(validate_secret("email", &serde_json::json!({ "to": "nope" })).is_err());
    }

    #[test]
    fn test_build_event_shape() {
        let event = build_event("deploy.failed", Some(7), "deployment", "42", "checkout", "boom");
        assert_eq!(event["eventType"], "deploy.failed");
        assert_eq!(event["projectId"], 7);
        assert_eq!(event["subject"]["kind"], "deployment");
        assert_eq!(event["dedupeKey"], "deploy.failed:42:boom");
    }
}
