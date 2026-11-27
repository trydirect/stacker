use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotificationChannel {
    pub id: i32,
    pub user_id: String,
    pub project_id: Option<i32>,
    pub name: String,
    pub channel_type: String,
    pub vault_path: String,
    pub config: serde_json::Value,
    pub events: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotificationDispatch {
    pub id: i32,
    pub channel_id: i32,
    pub event_type: String,
    pub project_id: Option<i32>,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
}

/// Event types and channel types live in the `notification-core` crate so its
/// unit tests run without building the service (see crates/notification-core).
pub use notification_core::{CHANNEL_TYPES, NOTIFICATION_EVENT_TYPES};
