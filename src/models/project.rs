use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: i32,         // id - is a unique identifier for the app project
    pub stack_id: Uuid,  // external project ID
    pub user_id: String, // external unique identifier for the user
    pub name: String,
    // pub body: sqlx::types::Json<String>,
    pub body: Value, //json type
    pub request_json: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn new(user_id: String, name: String, body: Value, request_json: Value) -> Self {
        Self {
            id: 0,
            stack_id: Uuid::new_v4(),
            user_id,
            name,
            body,
            request_json,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl Default for Project {
    fn default() -> Self {
        Project {
            id: 0,
            stack_id: Default::default(),
            user_id: "".to_string(),
            name: "".to_string(),
            body: Default::default(),
            request_json: Default::default(),
            created_at: Default::default(),
            updated_at: Default::default(),
        }
    }
}
