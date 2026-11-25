use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GithubInstallation {
    pub id: i32,
    pub user_id: String,
    pub installation_id: i64,
    pub account_login: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GithubRepo {
    pub id: i32,
    pub user_id: String,
    pub installation_id: i64,
    pub project_id: Option<i32>,
    pub repo: String,
    pub branch: String,
    pub watch_paths: Vec<String>,
    pub autodeploy: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GithubWebhookDelivery {
    pub delivery_id: String,
    pub event_type: String,
    pub received_at: DateTime<Utc>,
}
