use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChatConversation {
    pub id: Uuid,
    pub user_id: String,
    pub project_id: Option<i32>,
    pub messages: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A chat session (dialog) on the Stack Builder page. The message history lives
/// in `messages_encrypted` as base64(nonce || AES-256-GCM ciphertext) — never
/// serialized to clients directly. Use [`ChatSessionSummary`] for list/create
/// responses (no content) and decrypt on demand for the messages endpoint.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChatSession {
    pub id: Uuid,
    pub user_id: String,
    pub project_id: Option<i32>,
    pub title: Option<String>,
    #[serde(skip)] // ciphertext must never leak into an API response
    pub messages_encrypted: String,
    /// NULL = active; a timestamp = the session was archived (soft-closed).
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Content-free view of a chat session, safe to return from list/create.
/// Deliberately omits `messages_encrypted` so the list endpoint can never
/// expose (even encrypted) message content.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChatSessionSummary {
    pub id: Uuid,
    pub user_id: String,
    pub project_id: Option<i32>,
    pub title: Option<String>,
    /// NULL = active; a timestamp = archived. Lets the nav show/segregate
    /// archived threads without a second request.
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ChatSession> for ChatSessionSummary {
    fn from(s: ChatSession) -> Self {
        ChatSessionSummary {
            id: s.id,
            user_id: s.user_id,
            project_id: s.project_id,
            title: s.title,
            archived_at: s.archived_at,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}
