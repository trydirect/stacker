use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;
use serde::{Serialize,Deserialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Stack {
    pub id: i32,       // id - is a unique identifier for the app stack
    pub stack_id: Uuid, // external stack ID
    pub user_id: String,  // external unique identifier for the user
    pub name: String,
    // pub body: sqlx::types::Json<String>,
    pub body: Value, //json type
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
