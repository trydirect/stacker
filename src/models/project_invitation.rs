use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An invitation to join a project (Team seats).
///
/// `token_hash` never leaves the server: the token itself is only in the
/// invitation link sent to the invited address.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectInvitation {
    pub id: Uuid,
    pub project_id: i32,
    pub email: String,
    pub role: String,
    #[serde(skip)]
    pub token_hash: String,
    pub invited_by: String,
    pub invited_by_email: String,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub accepted_by: Option<String>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ProjectInvitation {
    /// The status a caller should see: a pending invitation past its expiry
    /// reads as expired even before anything rewrites the row.
    pub fn effective_status(&self) -> &str {
        if self.status == "pending" && self.expires_at <= Utc::now() {
            "expired"
        } else {
            &self.status
        }
    }
}

/// A pending invitation as the invited person sees it.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PendingProjectInvitation {
    pub id: Uuid,
    pub project_id: i32,
    pub project_name: String,
    pub role: String,
    pub invited_by: String,
    pub expires_at: DateTime<Utc>,
}
