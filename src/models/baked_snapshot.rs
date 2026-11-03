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
    /// The author-declared field policy (`ConfigContract`) captured at bake time,
    /// pinned to this image. The clone path reads it to regenerate
    /// `mutability: generated` fields fresh per buyer rather than inheriting the
    /// single value baked into the snapshot. `None` for snapshots baked before
    /// this column existed.
    pub config_contract: Option<serde_json::Value>,
    /// The `${VAR}` names the baked compose file references, captured at bake
    /// time. The clone path checks the buyer's env against this before creating
    /// a server: an unsatisfied reference resolves to an empty string at boot
    /// with nothing but a Compose warning to show for it.
    ///
    /// `None` for snapshots baked before this column existed — those skip the
    /// check rather than becoming undeployable.
    #[sqlx(default)]
    pub required_env_keys: Option<serde_json::Value>,
    /// Server type of the build box (e.g. `cpx42`). Quick Deploy provisions
    /// this type: the image carries that type's disk. `None` for snapshots
    /// baked before this column existed — the default type applies.
    #[sqlx(default)]
    pub server_type: Option<String>,
    /// Baked through the API: reaches buyers only once a channel (`latest` or
    /// `stable`) points at it. `false` for older rows and the `bake` binary's.
    #[sqlx(default)]
    pub gated: bool,
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
