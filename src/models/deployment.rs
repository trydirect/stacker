use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Store user deployment attempts for a specific project
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Deployment {
    pub id: i32,         // id - is a unique identifier for the app project
    pub project_id: i32,  // external project ID
    pub deployment_hash: String, // unique hash for agent identification
    pub user_id: Option<String>, // user who created the deployment (nullable in db)
    pub deleted: Option<bool>,
    pub status: String,
    pub metadata: Value, // renamed from 'body' to 'metadata'
    pub last_seen_at: Option<DateTime<Utc>>, // last heartbeat from agent
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Deployment {
    pub fn new(
        project_id: i32,
        user_id: Option<String>,
        deployment_hash: String,
        status: String,
        metadata: Value,
    ) -> Self {
        Self {
            id: 0,
            project_id,
            deployment_hash,
            user_id,
            deleted: Some(false),
            status,
            metadata,
            last_seen_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl Default for Deployment {
    fn default() -> Self {
        Deployment {
            id: 0,
            project_id: 0,
            deployment_hash: String::new(),
            user_id: None,
            deleted: Some(false),
            status: "pending".to_string(),
            metadata: Value::Null,
            last_seen_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
