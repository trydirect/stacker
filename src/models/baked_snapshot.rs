use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A published, health-gated server snapshot (immutable deploy). The `image_id`
/// is the Hetzner snapshot id that user deploys clone from.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BakedSnapshot {
    pub id: i32,
    pub stack: String,
    pub version: String,
    pub provider: String,
    pub image_id: i64,
    pub healthy: bool,
    /// JSON object mapping image ref -> digest (digest-pinned pulls at bake).
    pub digests: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl BakedSnapshot {
    /// The image_id to clone from, when this record is healthy.
    pub fn clone_image_id(&self) -> Option<i64> {
        if self.healthy {
            Some(self.image_id)
        } else {
            None
        }
    }
}
