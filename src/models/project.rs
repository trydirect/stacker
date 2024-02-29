use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: i32,         // id - is a unique identifier for the app project
    pub cloud_id: Option<i32>,   // cloud assigned to a project
    pub stack_id: Uuid,  // external project ID
    pub user_id: String, // external unique identifier for the user
    pub name: String,
    // pub body: sqlx::types::Json<String>,
    pub body: Value, //json type
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn new(user_id: String, name: String, body: Value) -> Self {
        Self {
            id: 0,
            stack_id: Uuid::new_v4(),
            cloud_id: None,
            user_id: user_id,
            name: name,
            body: body,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl Default for Project {
    fn default() -> Self {
        Project {
            user_id: "".to_string(),
            name: "".to_string(),
            ..Default::default()
        }
    }
}