use uuid::Uuid;
use chrono::{DateTime, Utc};

pub struct Stack {
    pub id: Uuid,       // id - is a unique identifier for the app stack
    pub stack_id: Uuid, // external stack ID
    pub user_id: Uuid,  // external unique identifier for the user
    pub name: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}


