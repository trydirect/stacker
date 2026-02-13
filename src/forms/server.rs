use crate::models;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct ServerForm {
    /// Reference to the cloud provider (DO, Hetzner, AWS, etc.)
    pub cloud_id: Option<i32>,
    pub region: Option<String>,
    pub zone: Option<String>,
    pub server: Option<String>,
    pub os: Option<String>,
    pub disk_type: Option<String>,
    pub srv_ip: Option<String>,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: Option<i32>,
    pub ssh_user: Option<String>,
    /// Optional friendly name for the server
    pub name: Option<String>,
    /// Connection mode: "ssh" or "password" or "status_panel"
    pub connection_mode: Option<String>,
    /// Path in Vault where SSH key is stored (e.g., "secret/users/{user_id}/servers/{server_id}/ssh")
    pub vault_key_path: Option<String>,
}

pub fn default_ssh_port() -> Option<i32> {
    Some(22)
}

impl From<&ServerForm> for models::Server {
    fn from(val: &ServerForm) -> Self {
        let mut server = models::Server::default();
        server.cloud_id = val.cloud_id;
        server.disk_type = val.disk_type.clone();
        server.region = val.region.clone();
        server.server = val.server.clone();
        server.zone = val.zone.clone();
        server.os = val.os.clone();
        server.created_at = Utc::now();
        server.updated_at = Utc::now();
        server.srv_ip = val.srv_ip.clone();
        server.ssh_port = val.ssh_port.clone().or_else(default_ssh_port);
        server.ssh_user = val.ssh_user.clone();
        server.name = val.name.clone();
        server.connection_mode = val
            .connection_mode
            .clone()
            .unwrap_or_else(|| "ssh".to_string());
        server.vault_key_path = val.vault_key_path.clone();

        server
    }
}

impl Into<ServerForm> for models::Server {
    fn into(self) -> ServerForm {
        let mut form = ServerForm::default();
        form.cloud_id = self.cloud_id;
        form.disk_type = self.disk_type;
        form.region = self.region;
        form.server = self.server;
        form.zone = self.zone;
        form.os = self.os;
        form.srv_ip = self.srv_ip;
        form.ssh_port = self.ssh_port;
        form.ssh_user = self.ssh_user;
        form.name = self.name;
        form.connection_mode = Some(self.connection_mode);
        form.vault_key_path = self.vault_key_path;

        form
    }
}
