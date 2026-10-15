use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A bake started through `POST /api/admin/bake`. `running` until the job ends
/// as `done` (with the snapshot) or `failed` (with the reason). The build box
/// (`server_id`, `server_ip`) is never removed by the job.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BakeJob {
    pub id: uuid::Uuid,
    pub stack: String,
    pub version: String,
    pub deployment_id: i32,
    pub server_id: i32,
    pub server_ip: String,
    pub status: String,
    pub image_id: Option<i64>,
    pub snapshot_id: Option<i32>,
    pub error: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
