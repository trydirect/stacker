use crate::models::{NotificationChannel, NotificationDispatch};
use sqlx::PgPool;

pub async fn insert_channel(
    pool: &PgPool,
    user_id: &str,
    project_id: Option<i32>,
    name: &str,
    channel_type: &str,
    vault_path: &str,
    config: &serde_json::Value,
    events: &[String],
    enabled: bool,
) -> Result<NotificationChannel, String> {
    sqlx::query_as::<_, NotificationChannel>(
        r#"
        INSERT INTO notification_channel
            (user_id, project_id, name, channel_type, vault_path, config, events, enabled)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(project_id)
    .bind(name)
    .bind(channel_type)
    .bind(vault_path)
    .bind(config)
    .bind(events)
    .bind(enabled)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to insert notification channel: {}", e))
}

pub async fn fetch_channel(
    pool: &PgPool,
    id: i32,
) -> Result<Option<NotificationChannel>, String> {
    sqlx::query_as::<_, NotificationChannel>(
        r#"
        SELECT *
        FROM notification_channel
        WHERE id = $1
        LIMIT 1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch notification channel: {}", e))
}

/// Channels of one user, optionally narrowed to one project
/// (project_id = None returns all of the user's channels).
pub async fn list_user_channels(
    pool: &PgPool,
    user_id: &str,
    project_id: Option<i32>,
) -> Result<Vec<NotificationChannel>, String> {
    sqlx::query_as::<_, NotificationChannel>(
        r#"
        SELECT *
        FROM notification_channel
        WHERE user_id = $1
          AND ($2::int IS NULL OR project_id = $2)
        ORDER BY name ASC
        "#,
    )
    .bind(user_id)
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list notification channels: {}", e))
}

/// Enabled channels of one project that receive its events. Global admin
/// channels are excluded: they get organization-level events only —
/// prevents cross-tenant event leaks (code-review B1).
pub async fn list_event_channels(
    pool: &PgPool,
    project_id: i32,
) -> Result<Vec<NotificationChannel>, String> {
    sqlx::query_as::<_, NotificationChannel>(
        r#"
        SELECT *
        FROM notification_channel
        WHERE enabled
          AND project_id = $1
        ORDER BY id ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list event notification channels: {}", e))
}

pub async fn update_channel(
    pool: &PgPool,
    id: i32,
    name: Option<&str>,
    config: Option<&serde_json::Value>,
    events: Option<&[String]>,
    enabled: Option<bool>,
) -> Result<NotificationChannel, String> {
    sqlx::query_as::<_, NotificationChannel>(
        r#"
        UPDATE notification_channel
        SET name = COALESCE($2, name),
            config = COALESCE($3, config),
            events = COALESCE($4, events),
            enabled = COALESCE($5, enabled),
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(config)
    .bind(events)
    .bind(enabled)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to update notification channel: {}", e))
}

pub async fn delete_channel(pool: &PgPool, id: i32) -> Result<(), String> {
    sqlx::query(
        r#"
        DELETE FROM notification_channel
        WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to delete notification channel: {}", e))
}

pub async fn insert_dispatch(
    pool: &PgPool,
    channel_id: i32,
    event_type: &str,
    project_id: Option<i32>,
    payload: &serde_json::Value,
) -> Result<NotificationDispatch, String> {
    sqlx::query_as::<_, NotificationDispatch>(
        r#"
        INSERT INTO notification_dispatch
            (channel_id, event_type, project_id, payload, status, attempts)
        VALUES ($1, $2, $3, $4, 'pending', 0)
        RETURNING *
        "#,
    )
    .bind(channel_id)
    .bind(event_type)
    .bind(project_id)
    .bind(payload)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to insert notification dispatch: {}", e))
}

pub async fn finish_dispatch(
    pool: &PgPool,
    id: i32,
    status: &str,
    attempts: i32,
    last_error: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        r#"
        UPDATE notification_dispatch
        SET status = $2,
            attempts = $3,
            last_error = $4,
            sent_at = CASE WHEN $2 = 'sent' THEN NOW() ELSE sent_at END
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(status)
    .bind(attempts)
    .bind(last_error)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to finish notification dispatch: {}", e))
}

/// Dispatches visible to one user (through their channels), newest first.
pub async fn list_dispatches(
    pool: &PgPool,
    user_id: &str,
    channel_id: Option<i32>,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<NotificationDispatch>, String> {
    sqlx::query_as::<_, NotificationDispatch>(
        r#"
        SELECT d.*
        FROM notification_dispatch d
        JOIN notification_channel c ON c.id = d.channel_id
        WHERE c.user_id = $1
          AND ($2::int IS NULL OR d.channel_id = $2)
          AND ($3::text IS NULL OR d.status = $3)
        ORDER BY d.id DESC
        LIMIT $4
        "#,
    )
    .bind(user_id)
    .bind(channel_id)
    .bind(status)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list notification dispatches: {}", e))
}
