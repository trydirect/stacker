//! Notification dispatch orchestration (WS-N) — the service layer over
//! `notification-core`. This module owns what the pure crate deliberately
//! does not: Vault secret retrieval, dispatch bookkeeping and the
//! project-event fan-out.
//!
//! Contracts: config/shared-fixtures/api-contracts/notifications-channels.json
//! and notifications-events.json. Plan: config/PLAN-notifications.md.
//!
//! Re-exports from `notification-core` keep the call sites (routes, tests)
//! on one import path.

use crate::db;
use crate::models::{NotificationChannel, NotificationDispatch};
use crate::services::VaultService;
use sqlx::PgPool;

pub use notification_core::{
    build_event, deploy_event_type, is_valid_channel_type, mask_target, redact_error,
    validate_secret, MAX_DELIVERY_ATTEMPTS,
};

/// Deliver an event to one channel with retry/backoff (at-least-once) and
/// record the dispatch row. Secrets come from Vault and never leave this
/// function unmasked.
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

    let dispatch = db::notification::insert_dispatch(pool, channel.id, event_type, project_id, event)
        .await?;

    let secret_raw = vault
        .fetch_secret_value(&channel.vault_path)
        .await
        .map_err(|e| format!("Failed to fetch channel secret from Vault: {}", e))?;
    let secret: serde_json::Value =
        serde_json::from_str(&secret_raw).map_err(|e| format!("Invalid channel secret: {}", e))?;

    match notification_core::deliver(&channel.channel_type, &secret, event).await {
        Ok(attempts) => {
            db::notification::finish_dispatch(pool, dispatch.id, "sent", attempts, None).await?;
            let mut done = dispatch.clone();
            done.status = "sent".to_string();
            done.attempts = attempts;
            done.sent_at = Some(chrono::Utc::now());
            Ok(done)
        }
        Err((attempts, last_error)) => {
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
/// tenant's project events (code-review B1). Channels whose `events` filter
/// does not include the event type are skipped.
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
        channel.project_id.map(i32::from).map(i64::from),
        "channel",
        &channel.id.to_string(),
        &channel.name,
        "Stacker test notification",
    );
    deliver_to_channel(pool, vault, channel, &event).await
}

/// WS-N N3: notify project channels about a deployment status transition.
/// Edge-triggered — fires only when the status actually changes into a
/// terminal state (see `deploy_event_type`). Best-effort: failures are
/// logged and never break the deploy flow.
pub async fn notify_deployment_status_change(
    pool: &PgPool,
    vault: &VaultService,
    deployment: &crate::models::Deployment,
    previous_status: &str,
    new_status: &str,
) {
    let Some(event_type) = deploy_event_type(previous_status, new_status) else {
        return;
    };
    let status_message = deployment
        .metadata
        .get("status_message")
        .and_then(|v| v.as_str())
        .unwrap_or(new_status)
        .to_string();
    let event = build_event(
        event_type,
        Some(i64::from(deployment.project_id)),
        "deployment",
        &deployment.deployment_hash,
        &deployment.deployment_hash,
        &format!(
            "Deployment {} {}: {}",
            deployment.deployment_hash, new_status, status_message
        ),
    );
    if let Err(error) = dispatch_project_event(pool, vault, deployment.project_id, &event).await {
        tracing::warn!(
            deployment_hash = %deployment.deployment_hash,
            "notification dispatch failed: {}", error
        );
    }
}

/// Convenience wrapper for console and route contexts: builds the Vault
/// client from the environment/configuration. Best-effort by design.
pub async fn notify_deployment_status_change_default(
    pool: &PgPool,
    deployment: &crate::models::Deployment,
    previous_status: &str,
    new_status: &str,
) {
    if deploy_event_type(previous_status, new_status).is_none() {
        return;
    }
    match VaultService::from_env() {
        Ok(Some(vault)) => {
            notify_deployment_status_change(pool, &vault, deployment, previous_status, new_status)
                .await
        }
        Ok(None) => {
            tracing::debug!("notification skipped: Vault is not configured");
        }
        Err(error) => {
            tracing::warn!("notification skipped: Vault unavailable: {}", error);
        }
    }
}
