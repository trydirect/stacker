use crate::db;
use crate::forms::{
    CreateNotificationChannelRequest, NotificationChannelResponse,
    NotificationDispatchResponse, UpdateNotificationChannelRequest,
};
use crate::helpers::JsonResponse;
use crate::models;
use crate::routes::project::access::{self, Need};
use crate::services::{notification as notify, VaultService};
use actix_web::{delete, get, patch, post, web, Responder, Result};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

/// Roles allowed to create organization-level (global) channels. The global
/// channel is an admin channel (decision N1); project channels need project
/// ownership instead (code-review B1).
const GLOBAL_CHANNEL_ROLES: [&str; 2] = ["root", "group_admin"];

fn build_vault(
    settings: &crate::configuration::Settings,
) -> Result<VaultService, actix_web::Error> {
    VaultService::from_settings(&settings.vault)
        .map_err(|error| JsonResponse::internal_server_error(error.to_string()))
}

/// Access to a channel: project channels belong to the project owner,
/// global channels to their creator. Unauthorized access answers 404
/// "Channel not found" so the status code does not confirm existence
/// (code-review m6).
async fn check_channel_access(
    pool: &PgPool,
    user: &models::User,
    project_id: Option<i32>,
    creator_id: &str,
) -> Result<(), actix_web::Error> {
    match project_id {
        Some(project_id) => {
            let project = db::project::fetch(pool, project_id)
                .await
                .map_err(JsonResponse::internal_server_error)?
                .ok_or_else(|| JsonResponse::not_found("Channel not found"))?;
            access::check(pool, &project, &user.id, Need::Owner, "Channel not found")
                .await
                .map(|_| ())
        }
        None if creator_id == user.id => Ok(()),
        None => Err(JsonResponse::not_found("Channel not found")),
    }
}

fn validate_events(events: &[String]) -> Result<(), actix_web::Error> {
    for event in events {
        if !models::NOTIFICATION_EVENT_TYPES.contains(&event.as_str()) {
            return Err(JsonResponse::bad_request(format!(
                "Unknown event type: {}",
                event
            )));
        }
    }
    Ok(())
}

fn map_insert_error(error: String) -> actix_web::Error {
    if error.contains("duplicate key") {
        JsonResponse::<String>::build().conflict("Channel name already exists in this scope")
    } else {
        JsonResponse::internal_server_error(error)
    }
}

#[tracing::instrument(name = "Create notification channel", skip_all)]
#[post("/channels")]
pub async fn create_handler(
    user: web::ReqData<Arc<models::User>>,
    body: web::Json<CreateNotificationChannelRequest>,
    pg_pool: web::Data<PgPool>,
    settings: web::Data<crate::configuration::Settings>,
) -> Result<impl Responder> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(JsonResponse::bad_request("name must not be empty"));
    }
    if !notify::is_valid_channel_type(&body.channel_type) {
        return Err(JsonResponse::bad_request(format!(
            "channelType must be one of {:?}",
            models::CHANNEL_TYPES
        )));
    }
    if let Err(error) = notify::validate_secret(&body.channel_type, &body.secret) {
        return Err(JsonResponse::bad_request(error));
    }
    let events = body.events.clone().unwrap_or_default();
    validate_events(&events)?;

    match body.project_id {
        Some(project_id) => {
            let project = db::project::fetch(pg_pool.get_ref(), project_id)
                .await
                .map_err(JsonResponse::internal_server_error)?
                .ok_or_else(|| JsonResponse::not_found("Project not found"))?;
            access::check(
                pg_pool.get_ref(),
                &project,
                &user.id,
                Need::Owner,
                "Project not found",
            )
            .await?;
        }
        None if GLOBAL_CHANNEL_ROLES.contains(&user.role.as_str()) => {}
        None => {
            return Err(JsonResponse::forbidden(
                "Global channels require an admin role",
            ))
        }
    }

    // Insert the row first: the Vault path is derived from (user, project,
    // name), so storing the secret first would overwrite an existing
    // channel's secret on a duplicate-name attempt (code-review M1).
    let vault = build_vault(settings.get_ref())?;
    let vault_path = vault.notification_secret_path(&user.id, body.project_id, &name);
    let channel = db::notification::insert_channel(
        pg_pool.get_ref(),
        &user.id,
        body.project_id,
        &name,
        &body.channel_type,
        &vault_path,
        body.config.as_ref().unwrap_or(&json!({})),
        &events,
        body.enabled.unwrap_or(true),
    )
    .await
    .map_err(map_insert_error)?;

    if let Err(error) = vault
        .store_secret_value(&vault_path, &body.secret.to_string())
        .await
    {
        // Do not leave a channel whose secret is missing.
        let _ = db::notification::delete_channel(pg_pool.get_ref(), channel.id).await;
        return Err(JsonResponse::internal_server_error(error.to_string()));
    }

    let hint = notify::mask_target(&body.secret, &body.channel_type);
    Ok(JsonResponse::build()
        .set_item(NotificationChannelResponse::from_channel(channel, hint))
        .created("Notification channel created"))
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(rename = "projectId")]
    pub project_id: Option<i32>,
}

#[tracing::instrument(name = "List notification channels", skip_all)]
#[get("/channels")]
pub async fn list_handler(
    user: web::ReqData<Arc<models::User>>,
    query: web::Query<ListQuery>,
    pg_pool: web::Data<PgPool>,
    settings: web::Data<crate::configuration::Settings>,
) -> Result<impl Responder> {
    let channels =
        db::notification::list_user_channels(pg_pool.get_ref(), &user.id, query.project_id)
            .await
            .map_err(JsonResponse::internal_server_error)?;

    let vault = build_vault(settings.get_ref())?;
    let mut items = Vec::with_capacity(channels.len());
    for channel in channels {
        let hint = match vault.fetch_secret_value(&channel.vault_path).await {
            Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(secret) => notify::mask_target(&secret, &channel.channel_type),
                Err(_) => "***".to_string(),
            },
            Err(_) => "***".to_string(),
        };
        items.push(NotificationChannelResponse::from_channel(channel, hint));
    }
    Ok(JsonResponse::build().set_list(items).ok("Notification channels"))
}

#[tracing::instrument(name = "Update notification channel", skip_all)]
#[patch("/channels/{id}")]
pub async fn update_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    body: web::Json<UpdateNotificationChannelRequest>,
    pg_pool: web::Data<PgPool>,
    settings: web::Data<crate::configuration::Settings>,
) -> Result<impl Responder> {
    let channel_id = path.into_inner();
    let channel = db::notification::fetch_channel(pg_pool.get_ref(), channel_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Channel not found"))?;

    check_channel_access(
        pg_pool.get_ref(),
        &user,
        channel.project_id,
        &channel.user_id,
    )
    .await?;

    if let Some(events) = &body.events {
        validate_events(events)?;
    }
    if let Some(secret) = &body.secret {
        if let Err(error) = notify::validate_secret(&channel.channel_type, secret) {
            return Err(JsonResponse::bad_request(error));
        }
    }

    let vault = build_vault(settings.get_ref())?;
    let mut hint = None;
    if let Some(secret) = &body.secret {
        vault
            .store_secret_value(&channel.vault_path, &secret.to_string())
            .await
            .map_err(|error| JsonResponse::internal_server_error(error.to_string()))?;
        hint = Some(notify::mask_target(secret, &channel.channel_type));
    }

    let updated = db::notification::update_channel(
        pg_pool.get_ref(),
        channel_id,
        body.name.as_deref().map(str::trim),
        body.config.as_ref(),
        body.events.as_deref(),
        body.enabled,
    )
    .await
    .map_err(map_insert_error)?;

    let hint = match hint {
        Some(hint) => hint,
        None => match vault.fetch_secret_value(&updated.vault_path).await {
            Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(secret) => notify::mask_target(&secret, &updated.channel_type),
                Err(_) => "***".to_string(),
            },
            Err(_) => "***".to_string(),
        },
    };
    Ok(JsonResponse::build()
        .set_item(NotificationChannelResponse::from_channel(updated, hint))
        .ok("Notification channel updated"))
}

#[tracing::instrument(name = "Delete notification channel", skip_all)]
#[delete("/channels/{id}")]
pub async fn delete_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    pg_pool: web::Data<PgPool>,
    settings: web::Data<crate::configuration::Settings>,
) -> Result<impl Responder> {
    let channel_id = path.into_inner();
    let channel = db::notification::fetch_channel(pg_pool.get_ref(), channel_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Channel not found"))?;

    check_channel_access(
        pg_pool.get_ref(),
        &user,
        channel.project_id,
        &channel.user_id,
    )
    .await?;

    db::notification::delete_channel(pg_pool.get_ref(), channel_id)
        .await
        .map_err(JsonResponse::internal_server_error)?;

    let vault = build_vault(settings.get_ref())?;
    if let Err(error) = vault.delete_secret_value(&channel.vault_path).await {
        // The row is gone; the orphaned secret is at least recorded (m2).
        tracing::warn!(
            channel_id,
            "notification channel deleted but its Vault secret could not be removed: {}",
            error
        );
    }

    Ok(JsonResponse::build()
        .set_item(json!({ "deleted": true }))
        .ok("Notification channel deleted"))
}

#[tracing::instrument(name = "Test notification channel", skip_all)]
#[post("/channels/{id}/test")]
pub async fn test_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    pg_pool: web::Data<PgPool>,
    settings: web::Data<crate::configuration::Settings>,
) -> Result<impl Responder> {
    let channel_id = path.into_inner();
    let channel = db::notification::fetch_channel(pg_pool.get_ref(), channel_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Channel not found"))?;

    check_channel_access(
        pg_pool.get_ref(),
        &user,
        channel.project_id,
        &channel.user_id,
    )
    .await?;

    let vault = build_vault(settings.get_ref())?;
    let dispatch = notify::send_test_message(pg_pool.get_ref(), &vault, &channel)
        .await
        .map_err(JsonResponse::internal_server_error)?;

    // 200 always carries the dispatch; the message reflects the outcome (m3).
    let message = if dispatch.status == "sent" {
        "Test notification sent"
    } else {
        "Test notification failed"
    };
    Ok(JsonResponse::build()
        .set_item(NotificationDispatchResponse::from(dispatch))
        .ok(message))
}

#[derive(Deserialize)]
pub struct DispatchListQuery {
    #[serde(rename = "channelId")]
    pub channel_id: Option<i32>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

#[tracing::instrument(name = "List notification dispatches", skip_all)]
#[get("/dispatches")]
pub async fn dispatch_list_handler(
    user: web::ReqData<Arc<models::User>>,
    query: web::Query<DispatchListQuery>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let items = db::notification::list_dispatches(
        pg_pool.get_ref(),
        &user.id,
        query.channel_id,
        query.status.as_deref(),
        limit,
    )
    .await
    .map_err(JsonResponse::internal_server_error)?
    .into_iter()
    .map(NotificationDispatchResponse::from)
    .collect::<Vec<_>>();

    Ok(JsonResponse::build()
        .set_list(items)
        .ok("Notification dispatches"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_events_accepts_known_types() {
        let events = vec!["deploy.failed".to_string(), "backup.succeeded".to_string()];
        assert!(validate_events(&events).is_ok());
    }

    #[test]
    fn test_validate_events_rejects_unknown_type() {
        let events = vec!["deploy.exploded".to_string()];
        assert!(validate_events(&events).is_err());
    }

    #[test]
    fn test_validate_events_accepts_empty_filter() {
        assert!(validate_events(&[]).is_ok());
    }
}
