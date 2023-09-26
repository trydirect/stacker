use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

// #[derive(sqlx::Type, Debug, Clone, Copy)]
// #[sqlx(rename_all = "lowercase", type_name = "json")]
#[derive(Debug)]
pub struct Stack {
    pub id: i32,       // id - is a unique identifier for the app stack
    pub stack_id: Uuid, // external stack ID
    pub user_id: i32,  // external unique identifier for the user
    pub name: String,
    // pub body: sqlx::types::Json<String>,
    pub body: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}


