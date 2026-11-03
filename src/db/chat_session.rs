use crate::models::{ChatSession, ChatSessionSummary};
use sqlx::PgPool;
use uuid::Uuid;

const SESSION_COLUMNS: &str =
    "id, user_id, project_id, title, messages_encrypted, archived_at, created_at, updated_at";

/// Which sessions a list query should return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchivedFilter {
    /// Active (non-archived) sessions only — the default nav view.
    Active,
    /// Archived (soft-closed) sessions only — the "recent"/archive view.
    Archived,
}

impl ArchivedFilter {
    /// Map an optional `?archived=` query flag to a filter (default: Active).
    pub fn from_query(archived: Option<bool>) -> Self {
        match archived {
            Some(true) => ArchivedFilter::Archived,
            _ => ArchivedFilter::Active,
        }
    }

    fn wants_archived(self) -> bool {
        matches!(self, ArchivedFilter::Archived)
    }
}

/// Create a new chat session for `user_id`. `messages_encrypted` is the already
/// AES-256-GCM encrypted, base64-encoded blob (empty string = empty session).
pub async fn create(
    pool: &PgPool,
    user_id: &str,
    project_id: Option<i32>,
    title: Option<&str>,
    messages_encrypted: &str,
) -> Result<ChatSession, sqlx::Error> {
    sqlx::query_as::<_, ChatSession>(&format!(
        "INSERT INTO chat_sessions (user_id, project_id, title, messages_encrypted) \
         VALUES ($1, $2, $3, $4) RETURNING {SESSION_COLUMNS}"
    ))
    .bind(user_id)
    .bind(project_id)
    .bind(title)
    .bind(messages_encrypted)
    .fetch_one(pool)
    .await
}

/// List a user's sessions newest-first. Selects metadata only — the encrypted
/// message blob is never read here, so the list response cannot leak content.
/// Filters by archived state (default: active) and, when `project_id` is
/// `Some`, by project.
pub async fn list(
    pool: &PgPool,
    user_id: &str,
    project_id: Option<i32>,
    archived: ArchivedFilter,
) -> Result<Vec<ChatSessionSummary>, sqlx::Error> {
    // `$3` selects active vs archived without four hand-written branches:
    //   want_archived = false -> archived_at IS NULL     (active)
    //   want_archived = true  -> archived_at IS NOT NULL  (archived)
    sqlx::query_as::<_, ChatSessionSummary>(
        r#"SELECT id, user_id, project_id, title, archived_at, created_at, updated_at
           FROM chat_sessions
           WHERE user_id = $1
             AND ($2::int IS NULL OR project_id = $2::int)
             AND ((archived_at IS NOT NULL) = $3)
           ORDER BY updated_at DESC"#,
    )
    .bind(user_id)
    .bind(project_id)
    .bind(archived.wants_archived())
    .fetch_all(pool)
    .await
}

/// Fetch a single session scoped to its owner. Returns `None` when the session
/// does not exist OR belongs to another user — callers surface that as a 404,
/// so a session's existence is never leaked across users.
pub async fn fetch(
    pool: &PgPool,
    id: &Uuid,
    user_id: &str,
) -> Result<Option<ChatSession>, sqlx::Error> {
    sqlx::query_as::<_, ChatSession>(&format!(
        "SELECT {SESSION_COLUMNS} FROM chat_sessions WHERE id = $1 AND user_id = $2"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// Replace a session's encrypted message blob (used by the append handler,
/// which decrypts, pushes, and re-encrypts). Owner-scoped. Returns the updated
/// row, or `None` when the session does not exist / belongs to another user.
pub async fn update_messages(
    pool: &PgPool,
    id: &Uuid,
    user_id: &str,
    messages_encrypted: &str,
) -> Result<Option<ChatSession>, sqlx::Error> {
    sqlx::query_as::<_, ChatSession>(&format!(
        "UPDATE chat_sessions SET messages_encrypted = $3, updated_at = NOW() \
         WHERE id = $1 AND user_id = $2 RETURNING {SESSION_COLUMNS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(messages_encrypted)
    .fetch_optional(pool)
    .await
}

/// Update a session's title. Owner-scoped. Returns the updated row, or `None`
/// when the session does not exist / belongs to another user.
pub async fn update_title(
    pool: &PgPool,
    id: &Uuid,
    user_id: &str,
    title: Option<&str>,
) -> Result<Option<ChatSession>, sqlx::Error> {
    sqlx::query_as::<_, ChatSession>(&format!(
        "UPDATE chat_sessions SET title = $3, updated_at = NOW() \
         WHERE id = $1 AND user_id = $2 RETURNING {SESSION_COLUMNS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(title)
    .fetch_optional(pool)
    .await
}

/// Archive (soft-close) or unarchive a session. Owner-scoped. Setting
/// `archived = true` stamps `archived_at = NOW()`; `false` clears it. Returns
/// the updated row, or `None` when the session does not exist / isn't the
/// caller's. Note: `updated_at` is intentionally NOT bumped, so archiving does
/// not reorder the active list or the archived list unexpectedly.
pub async fn set_archived(
    pool: &PgPool,
    id: &Uuid,
    user_id: &str,
    archived: bool,
) -> Result<Option<ChatSession>, sqlx::Error> {
    sqlx::query_as::<_, ChatSession>(&format!(
        "UPDATE chat_sessions \
         SET archived_at = CASE WHEN $3 THEN NOW() ELSE NULL END \
         WHERE id = $1 AND user_id = $2 RETURNING {SESSION_COLUMNS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(archived)
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
