use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cloud {
    pub id: i32,
    pub user_id: String,
    pub provider: String,
    pub cloud_token: Option<String>,
    pub cloud_key: Option<String>,
    pub cloud_secret: Option<String>,
    pub save_token: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
