//! Notification channel adapters and dispatch (WS-N).
//!
//! Contracts: config/shared-fixtures/api-contracts/notifications-channels.json
//! and notifications-events.json. Plan: config/PLAN-notifications.md.
//!
//! Secrets (webhook urls, bot tokens) live only in Vault; errors are redacted
//! before they are stored or returned.

use crate::db;
use crate::models::{NotificationChannel, NotificationDispatch, CHANNEL_TYPES};
use crate::services::VaultService;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use std::sync::OnceLock;
use std::time::Duration;

pub const MAX_DELIVERY_ATTEMPTS: i32 = 3;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Shared HTTP client with a request timeout (code-review M2/M8): one pool,
/// no unbounded hangs on a wedged webhook endpoint.
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

#[derive(Debug)]
struct SendFailure {
    retryable: bool,
    message: String,
}

pub fn is_valid_channel_type(channel_type: &str) -> bool {
    CHANNEL_TYPES.contains(&channel_type)
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
                Some((local, domain)) => format!("{}***@{}", local.chars().next().unwrap_or('*'), domain),
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
/// Walks nested JSON values as well (code-review m5).
pub fn redact_error(error: &str, secret: &serde_json::Value) -> String {
    fn collect_strings(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
            serde_json::Value::Object(map) => {
                map.values().for_each(|v| collect_strings(v, out))
            }
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

/// Basic secret-shape validation (code-review m4 partial): url-based types
/// must carry an http(s) URL. SSRF hardening (private-range blocking) is
/// tracked in config/PLAN-notifications.md.
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

fn build_event(
    event_type: &str,
    project_id: Option<i32>,
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

async fn send_once(
    channel_type: &str,
    secret: &serde_json::Value,
    event: &serde_json::Value,
) -> Result<(), SendFailure> {
    let message = event
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("Stacker notification")
        .to_string();

    let request = match channel_type {
        "slack" => {
            let url = secret
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SendFailure {
                    retryable: false,
                    message: "slack secret must contain url".to_string(),
                })?;
            http_client()
                .post(url)
                .json(&serde_json::json!({ "text": message }))
        }
        "discord" => {
            let url = secret
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SendFailure {
                    retryable: false,
                    message: "discord secret must contain url".to_string(),
                })?;
            http_client()
                .post(url)
                .json(&serde_json::json!({ "content": message }))
        }
        "telegram" => {
            let token = secret
                .get("botToken")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SendFailure {
                    retryable: false,
                    message: "telegram secret must contain botToken".to_string(),
                })?;
            let chat_id = secret
                .get("chatId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SendFailure {
                    retryable: false,
                    message: "telegram secret must contain chatId".to_string(),
                })?;
            http_client()
                .post(format!("https://api.telegram.org/bot{}/sendMessage", token))
                .json(&serde_json::json!({ "chat_id": chat_id, "text": message }))
        }
        "webhook" => {
            let url = secret
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SendFailure {
                    retryable: false,
                    message: "webhook secret must contain url".to_string(),
                })?;
            let mut request = http_client().post(url).json(event);
            if let Some(key) = secret.get("hmacKey").and_then(|v| v.as_str()) {
                let body = serde_json::to_string(event).unwrap_or_default();
                let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).map_err(|e| {
                    SendFailure {
                        retryable: false,
                        message: format!("invalid hmac key: {}", e),
                    }
                })?;
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
            // Email goes through RabbitMQ -> notify service (step N4 of
            // PLAN-notifications.md); direct SMTP delivery is not used here.
            return Err(SendFailure {
                retryable: false,
                message: "email delivery goes through the notify service (N4)".to_string(),
            });
        }
        other => {
            return Err(SendFailure {
                retryable: false,
                message: format!("unsupported channel type: {}", other),
            })
        }
    };

    let response = request.send().await.map_err(|e| SendFailure {
        retryable: true,
        message: format!("request failed: {}", e),
    })?;

    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let retryable = status.as_u16() == 429 || status.is_server_error();
    let body = response.text().await.unwrap_or_default();
    Err(SendFailure {
        retryable,
        message: format!("HTTP {}: {}", status, body),
    })
}

/// Deliver an event to one channel with retry/backoff on 429/5xx
/// (at-least-once, see the events contract). Records the dispatch row.
pub async fn deliver_to_channel(
    pool: &PgPool,
    vault: &VaultService,
    channel: &NotificationChannel,
    event: &serde_json::Value,
) -> Result<NotificationDispatch, String> {
    let event_type = event
        .get("eventType")
        .and_then(|v| v.as_str())
        .unwrap_or("channel.test");
    let project_id = event
        .get("projectId")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);

    let dispatch = db::notification::insert_dispatch(
        pool,
        channel.id,
        event_type,
        project_id,
        event,
    )
    .await?;

    let secret_raw = vault
        .fetch_secret_value(&channel.vault_path)
        .await
        .map_err(|e| format!("Failed to fetch channel secret from Vault: {}", e))?;
    let secret: serde_json::Value =
        serde_json::from_str(&secret_raw).map_err(|e| format!("Invalid channel secret: {}", e))?;

    let mut attempts = 0;
    let outcome = loop {
        attempts += 1;
        match send_once(&channel.channel_type, &secret, event).await {
            Ok(()) => break Ok(()),
            Err(failure) => {
                let message = redact_error(&failure.message, &secret);
                if !failure.retryable || attempts >= MAX_DELIVERY_ATTEMPTS {
                    break Err(message);
                }
                tokio::time::sleep(Duration::from_millis(500 * attempts as u64)).await;
            }
        }
    };

    match outcome {
        Ok(()) => {
            db::notification::finish_dispatch(pool, dispatch.id, "sent", attempts, None).await?;
            let mut done = dispatch.clone();
            done.status = "sent".to_string();
            done.attempts = attempts;
            done.sent_at = Some(chrono::Utc::now());
            Ok(done)
        }
        Err(last_error) => {
            db::notification::finish_dispatch(
                pool,
                dispatch.id,
                "failed",
                attempts,
                Some(&last_error),
            )
            .await?;
            let mut done = dispatch.clone();
            done.status = "failed".to_string();
            done.attempts = attempts;
            done.last_error = Some(last_error);
            Ok(done)
        }
    }
}

/// Dispatch an event to the project's subscribed channels (per-project
/// channels only). Global admin channels deliberately do NOT receive project
/// events: they are reserved for organization-level events (decision N1) —
/// this also prevents one tenant's global channel from seeing another
/// tenant's project events (code-review B1).
pub async fn dispatch_project_event(
    pool: &PgPool,
    vault: &VaultService,
    project_id: i32,
    event: &serde_json::Value,
) -> Result<Vec<NotificationDispatch>, String> {
    let event_type = event
        .get("eventType")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let channels = db::notification::list_event_channels(pool, project_id).await?;
    let mut results = Vec::new();
    for channel in channels {
        if !channel.events.is_empty() && !channel.events.contains(&event_type) {
            continue;
        }
        results.push(deliver_to_channel(pool, vault, &channel, event).await?);
    }
    Ok(results)
}

/// Build and deliver a `channel.test` event to one channel (API: channel-test).
pub async fn send_test_message(
    pool: &PgPool,
    vault: &VaultService,
    channel: &NotificationChannel,
) -> Result<NotificationDispatch, String> {
    let event = build_event(
        "channel.test",
        channel.project_id,
        "channel",
        &channel.id.to_string(),
        &channel.name,
        "Stacker test notification",
    );
    deliver_to_channel(pool, vault, channel, &event).await
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
    fn test_redact_error_truncates_long_output() {
        let secret = serde_json::json!({ "url": "https://example.com/hook" });
        let error = "x".repeat(2000);
        let redacted = redact_error(&error, &secret);
        assert!(redacted.chars().count() <= 500);
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
