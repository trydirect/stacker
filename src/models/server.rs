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
    /// Path in Vault where SSH key is stored (e.g., "users/{user_id}/servers/{server_id}/ssh")
    pub vault_key_path: Option<String>,
    /// Connection mode: "ssh" (default) or "password"
    #[serde(default = "default_connection_mode")]
    pub connection_mode: String,
    /// SSH key status: "none", "pending", "active", "failed"
    #[serde(default = "default_key_status")]
    pub key_status: String,
    /// Optional friendly name for the server
    #[validate(max_length = 100)]
    pub name: Option<String>,
}

fn default_connection_mode() -> String {
    "ssh".to_string()
}

fn default_key_status() -> String {
    "none".to_string()
}
