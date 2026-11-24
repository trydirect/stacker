//! Notification dispatch orchestration (WS-N) — the service layer over
//! `notification-core`. This module owns what the pure crate deliberately
//! does not: Vault secret retrieval, MQ email delivery, dispatch bookkeeping
//! and the project-event fan-out.
//!
//! Contracts: config/shared-fixtures/api-contracts/notifications-channels.json
//! and notifications-events.json. Plan: config/PLAN-notifications.md.
//!
//! Re-exports from `notification-core` keep the call sites (routes, tests)
//! on one import path.

use crate::db;
use crate::helpers::MqManager;
use crate::models::{NotificationChannel, NotificationDispatch};
use crate::services::VaultService;
use serde::Serialize;
use sqlx::PgPool;

pub use notification_core::{
    build_event, deploy_event_type, is_valid_channel_type, mask_target, redact_error,
    validate_secret, MAX_DELIVERY_ATTEMPTS,
};

/// Message payload for the notify service email channel (same shape as
/// `project_cleanup_notifier::NotifyEmailMessage`).
#[derive(Debug, Serialize)]
struct NotifyEmailMessage {
    to: String,
    subject: String,
    message: String,
    html: Option<String>,
}

/// Email goes through RabbitMQ -> the notify service (N4). Routing key
/// follows the established `notify.start.email.<purpose>.all` convention.
const EMAIL_ROUTING_KEY: &str = "notify.start.email.notification.all";

async fn deliver_email(
    mq: &MqManager,
    secret: &serde_json::Value,
    event: &serde_json::Value,
) -> Result<(), String> {
    let to = secret
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "email secret must contain to".to_string())?;
    let message = event
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("Stacker notification")
        .to_string();
    let msg = NotifyEmailMessage {
        to: to.to_string(),
        subject: message.clone(),
        message,
        html: None,
    };
    mq.publish("notify".to_string(), EMAIL_ROUTING_KEY.to_string(), &msg)
        .await
        .map(|_| ())
        .map_err(|e| format!("notify service publish failed: {}", e))
}

/// Deliver an event to one channel with retry/backoff (at-least-once) and
/// record the dispatch row. Secrets come from Vault and never leave this
/// function unmasked. Email channels are delivered through the notify
/// service (`mq` is required for them).
pub async fn deliver_to_channel(
    pool: &PgPool,
    mq: Option<&MqManager>,
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

    let outcome = if channel.channel_type == "email" {
        match mq {
            Some(mq) => deliver_email(mq, &secret, event)
                .await
                .map(|_| 1)
                .map_err(|e| (1, e)),
            None => Err((
                1,
                "email delivery requires the notify service (MQ unavailable in this context)"
                    .to_string(),
            )),
        }
    } else {
        notification_core::deliver(&channel.channel_type, &secret, event).await
    };

    match outcome {
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
    mq: Option<&MqManager>,
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
        results.push(deliver_to_channel(pool, mq, vault, &channel, event).await?);
    }
    Ok(results)
}

/// Build and deliver a `channel.test` event to one channel (API: channel-test).
pub async fn send_test_message(
    pool: &PgPool,
    mq: Option<&MqManager>,
    vault: &VaultService,
    channel: &NotificationChannel,
) -> Result<NotificationDispatch, String> {
    let event = build_event(
        "channel.test",
        channel.project_id.map(i64::from),
        "channel",
        &channel.id.to_string(),
        &channel.name,
        "Stacker test notification",
    );
    deliver_to_channel(pool, mq, vault, channel, &event).await
}

/// WS-N N3: notify project channels about a deployment status transition.
/// Edge-triggered — fires only when the status actually changes into a
/// terminal state (see `deploy_event_type`). Best-effort: failures are
/// logged and never break the deploy flow.
pub async fn notify_deployment_status_change(
    pool: &PgPool,
    mq: Option<&MqManager>,
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
    if let Err(error) =
        dispatch_project_event(pool, mq, vault, deployment.project_id, &event).await
    {
        tracing::warn!(
            deployment_hash = %deployment.deployment_hash,
            "notification dispatch failed: {}", error
        );
    }
}

struct DefaultContext {
    vault: Option<VaultService>,
    mq: Option<MqManager>,
}

fn default_context() -> DefaultContext {
    let vault = match VaultService::from_env() {
        Ok(vault) => vault,
        Err(error) => {
            tracing::warn!("notification: Vault unavailable: {}", error);
            None
        }
    };
    let mq = match crate::configuration::get_configuration() {
        Ok(settings) => match MqManager::try_new(settings.amqp.connection_string()) {
            Ok(mq) => Some(mq),
            Err(error) => {
                tracing::warn!("notification: MQ unavailable: {}", error);
                None
            }
        },
        Err(error) => {
            tracing::warn!("notification: configuration unavailable: {}", error);
            None
        }
    };
    DefaultContext { vault, mq }
}

/// Convenience wrapper for console and route contexts: builds the Vault and
/// MQ clients from configuration. Best-effort by design.
pub async fn notify_deployment_status_change_default(
    pool: &PgPool,
    deployment: &crate::models::Deployment,
    previous_status: &str,
    new_status: &str,
) {
    if deploy_event_type(previous_status, new_status).is_none() {
        return;
    }
    let context = default_context();
    let Some(vault) = context.vault.as_ref() else {
        return;
    };
    notify_deployment_status_change(
        pool,
        context.mq.as_ref(),
        vault,
        deployment,
        previous_status,
        new_status,
    )
    .await
}

/// Convenience wrapper for fanning out an arbitrary project event (used by
/// the agent report path for `container.down`). Best-effort by design.
pub async fn dispatch_project_event_default(
    pool: &PgPool,
    project_id: i32,
    event: &serde_json::Value,
) {
    let context = default_context();
    let Some(vault) = context.vault.as_ref() else {
        return;
    };
    if let Err(error) =
        dispatch_project_event(pool, context.mq.as_ref(), vault, project_id, event).await
    {
        tracing::warn!("notification dispatch failed: {}", error);
    }
}

/// WS-N N4: fire `container.down` for containers that just transitioned from
/// `running` to `exited`/`dead` between two aggregate health reports.
/// Edge-triggered and best-effort: never fails the agent's report.
pub async fn notify_container_down_transitions(
    pool: &PgPool,
    deployment_hash: &str,
    previous: &[crate::models::DeploymentContainer],
    observed: &[crate::models::ObservedContainer],
) {
    let down: Vec<&crate::models::ObservedContainer> = observed
        .iter()
        .filter(|container| {
            let was_running = previous
                .iter()
                .find(|row| row.container_name == container.container_name)
                .map(|row| row.state.as_deref() == Some("running"))
                .unwrap_or(false);
            let is_down = matches!(
                container.state.as_deref(),
                Some("exited") | Some("dead")
            );
            was_running && is_down
        })
        .collect();

    if down.is_empty() {
        return;
    }

    let Some(deployment) = db::deployment::fetch_by_deployment_hash(pool, deployment_hash)
        .await
        .ok()
        .flatten()
    else {
        return;
    };

    for container in down {
        let name = container.container_name.clone();
        let label = container.app_code.clone().unwrap_or_else(|| name.clone());
        let event = build_event(
            "container.down",
            Some(i64::from(deployment.project_id)),
            "container",
            &name,
            &label,
            &format!(
                "Container {} of {} is down ({})",
                name,
                label,
                container.state.as_deref().unwrap_or("unknown")
            ),
        );
        dispatch_project_event_default(pool, deployment.project_id, &event).await;
    }
}
