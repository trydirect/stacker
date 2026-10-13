use serde::Serialize;
use sqlx::{PgPool, Row};

/// Agreement name that gates admin access to chat content.
pub const AI_CHAT_AGREEMENT: &str = "ai_chat_analysis";

#[derive(Debug, Serialize)]
pub struct ConsentedChat {
    pub source: &'static str,
    pub id: String,
    pub user_id: String,
    pub title: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Encrypted blob for `chat_sessions`; empty for `chat_conversations`.
    #[serde(skip)]
    pub messages_encrypted: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plain_messages: Option<serde_json::Value>,
}

/// Most recently updated chats of users who accepted the AI chat analysis
/// agreement, from both `chat_sessions` and `chat_conversations`.
pub async fn recent_consented(pool: &PgPool, limit: i64) -> Result<Vec<ConsentedChat>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT * FROM (
            SELECT 'session' AS source, s.id::text AS id, s.user_id, s.title,
                   s.updated_at, s.messages_encrypted, NULL::jsonb AS plain_messages
            FROM chat_sessions s
            UNION ALL
            SELECT 'conversation', c.id::text, c.user_id, NULL, c.updated_at, '', c.messages
            FROM chat_conversations c
        ) chats
        WHERE EXISTS (
            SELECT 1 FROM user_agreement ua
            JOIN agreement a ON a.id = ua.agrt_id
            WHERE a.name = $1 AND ua.user_id = chats.user_id
        )
        ORDER BY updated_at DESC
        LIMIT $2
        "#,
    )
    .bind(AI_CHAT_AGREEMENT)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let source: String = r.try_get("source")?;
            Ok(ConsentedChat {
                source: if source == "session" { "session" } else { "conversation" },
                id: r.try_get("id")?,
                user_id: r.try_get("user_id")?,
                title: r.try_get("title")?,
                updated_at: r.try_get("updated_at")?,
                messages_encrypted: r.try_get("messages_encrypted")?,
                plain_messages: r.try_get("plain_messages")?,
            })
        })
        .collect()
}
