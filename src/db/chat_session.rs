use crate::models::{ChatSession, ChatSessionSummary};
use sqlx::PgPool;
use uuid::Uuid;

/// Create a new chat session for `user_id`. `messages_encrypted` is the already
/// AES-256-GCM encrypted, base64-encoded blob (empty string = empty session).
pub async fn create(
    pool: &PgPool,
    user_id: &str,
    project_id: Option<i32>,
    title: Option<&str>,
    messages_encrypted: &str,
) -> Result<ChatSession, sqlx::Error> {
    sqlx::query_as::<_, ChatSession>(
        r#"INSERT INTO chat_sessions (user_id, project_id, title, messages_encrypted)
           VALUES ($1, $2, $3, $4)
           RETURNING id, user_id, project_id, title, messages_encrypted, created_at, updated_at"#,
    )
    .bind(user_id)
    .bind(project_id)
    .bind(title)
    .bind(messages_encrypted)
    .fetch_one(pool)
    .await
}

/// List a user's sessions newest-first. Selects metadata only — the encrypted
/// message blob is never read here, so the list response cannot leak content.
/// When `project_id` is `Some`, results are filtered to that project.
pub async fn list(
    pool: &PgPool,
    user_id: &str,
    project_id: Option<i32>,
) -> Result<Vec<ChatSessionSummary>, sqlx::Error> {
    match project_id {
        Some(pid) => {
            sqlx::query_as::<_, ChatSessionSummary>(
                r#"SELECT id, user_id, project_id, title, created_at, updated_at
                   FROM chat_sessions
                   WHERE user_id = $1 AND project_id = $2
                   ORDER BY updated_at DESC"#,
            )
            .bind(user_id)
            .bind(pid)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query_as::<_, ChatSessionSummary>(
                r#"SELECT id, user_id, project_id, title, created_at, updated_at
                   FROM chat_sessions
                   WHERE user_id = $1
                   ORDER BY updated_at DESC"#,
            )
            .bind(user_id)
            .fetch_all(pool)
            .await
        }
    }
}

/// Fetch a single session scoped to its owner. Returns `None` when the session
/// does not exist OR belongs to another user — callers surface that as a 404,
/// so a session's existence is never leaked across users.
pub async fn fetch(
    pool: &PgPool,
    id: &Uuid,
    user_id: &str,
) -> Result<Option<ChatSession>, sqlx::Error> {
    sqlx::query_as::<_, ChatSession>(
        r#"SELECT id, user_id, project_id, title, messages_encrypted, created_at, updated_at
           FROM chat_sessions
           WHERE id = $1 AND user_id = $2"#,
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// Delete a session scoped to its owner. Returns the number of rows removed
/// (0 when the session does not exist or is owned by another user).
pub async fn delete(pool: &PgPool, id: &Uuid, user_id: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(r#"DELETE FROM chat_sessions WHERE id = $1 AND user_id = $2"#)
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
