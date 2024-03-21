use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Store user deployment attempts for a specific project
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Deployment {
    pub id: i32,         // id - is a unique identifier for the app project
    pub project_id: i32,  // external project ID
    pub deleted: Option<bool>,
    pub status: String,
    pub body: Value, //json type
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Deployment {
    pub fn new(project_id: i32, status: String, body: Value) -> Self {
        Self {
            id: 0,
            project_id,
            deleted: Some(false),
            status,
            body,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl Default for Deployment {
    fn default() -> Self {
        Deployment {
            status: "pending".to_string(),
            ..Default::default()
        }
    }
}
