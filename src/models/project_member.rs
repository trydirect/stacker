use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectMember {
    pub project_id: i32,
    pub user_id: String,
    pub role: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A member as the owner sees them: the address comes from the invitation
/// they accepted; members added by user id before invitations have none.
#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct ProjectMemberWithEmail {
    pub project_id: i32,
    pub user_id: String,
    pub role: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub email: Option<String>,
}
