use chrono::{DateTime, Utc};
use serde_derive::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Server {
    pub id: i32,
    pub user_id: String,
    pub project_id: i32,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub region: Option<String>,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub zone: Option<String>,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub server: Option<String>,
    #[validate(min_length = 2)]
    #[validate(max_length = 50)]
    pub os: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub disk_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[validate(min_length = 8)]
    #[validate(max_length = 50)]
    pub srv_ip: Option<String>,
    #[validate(minimum = 20)]
    #[validate(maximum = 65535)]
    pub ssh_port: Option<i32>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub ssh_user: Option<String>,
}
