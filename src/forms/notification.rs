use crate::models::{NotificationChannel, NotificationDispatch};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct CreateNotificationChannelRequest {
    pub name: String,
    #[serde(rename = "projectId")]
    pub project_id: Option<i32>,
    #[serde(rename = "channelType")]
    pub channel_type: String,
    /// Write-only channel secret (webhook url / bot token / email target).
    /// Stored in Vault, never returned.
    pub secret: serde_json::Value,
    pub config: Option<serde_json::Value>,
    pub events: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateNotificationChannelRequest {
    pub name: Option<String>,
    /// Write-only; blank/omitted keeps the stored value.
    pub secret: Option<serde_json::Value>,
    pub config: Option<serde_json::Value>,
    pub events: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationChannelResponse {
    pub id: i32,
    pub name: String,
    #[serde(rename = "projectId")]
    pub project_id: Option<i32>,
    #[serde(rename = "channelType")]
    pub channel_type: String,
    /// Masked target for display; never the full secret.
    #[serde(rename = "targetHint")]
    pub target_hint: String,
    pub config: serde_json::Value,
    pub events: Vec<String>,
    pub enabled: bool,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

impl NotificationChannelResponse {
    pub fn from_channel(channel: NotificationChannel, target_hint: String) -> Self {
        Self {
            id: channel.id,
            name: channel.name,
            project_id: channel.project_id,
            channel_type: channel.channel_type,
            target_hint,
            config: channel.config,
            events: channel.events,
            enabled: channel.enabled,
            created_at: channel.created_at,
            updated_at: channel.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationDispatchResponse {
    pub id: i32,
    #[serde(rename = "channelId")]
    pub channel_id: i32,
    #[serde(rename = "eventType")]
    pub event_type: String,
    #[serde(rename = "projectId")]
    pub project_id: Option<i32>,
    pub status: String,
    pub attempts: i32,
    #[serde(rename = "lastError")]
    pub last_error: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "sentAt")]
    pub sent_at: Option<DateTime<Utc>>,
}

impl From<NotificationDispatch> for NotificationDispatchResponse {
    fn from(d: NotificationDispatch) -> Self {
        Self {
            id: d.id,
            channel_id: d.channel_id,
            event_type: d.event_type,
            project_id: d.project_id,
            status: d.status,
            attempts: d.attempts,
            last_error: d.last_error,
            created_at: d.created_at,
            sent_at: d.sent_at,
        }
    }
}
