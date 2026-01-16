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
    // pub metadata: sqlx::types::Json<String>,
    pub metadata: Value, //json type
    pub request_json: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub source_template_id: Option<Uuid>, // marketplace template UUID
    pub template_version: Option<String>, // marketplace template version
}

impl Project {
    pub fn new(user_id: String, name: String, metadata: Value, request_json: Value) -> Self {
        Self {
            id: 0,
            stack_id: Uuid::new_v4(),
            user_id,
            name,
            metadata,
            request_json,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            source_template_id: None,
            template_version: None,
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
            metadata: Default::default(),
            request_json: Default::default(),
            created_at: Default::default(),
            updated_at: Default::default(),
            source_template_id: None,
            template_version: None,
        }
    }
}
