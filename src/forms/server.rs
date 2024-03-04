use crate::models;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use chrono::{DateTime, Utc};
use crate::db;
#[derive(Serialize, Deserialize, Debug, Validate)]
pub struct Server {
    pub user_id: String,
    pub cloud_id: i32,
    pub project_id: i32,
    pub region: String,
    pub zone: Option<String>,
    pub server: String,
    pub os: String,
    pub disk_type: Option<String>,
}

impl Into<models::Server> for Server {
    fn into(self) -> models::Server {
        let mut server = models::Server::default();
        server.user_id = self.user_id;
        server.cloud_id = self.cloud_id;
        server.project_id = self.project_id;
        server.disk_type = self.disk_type;
        server.region = self.region;
        server.server = self.server;
        server.zone = self.zone;
        server.os = self.os;
        server.created_at = Utc::now();
        server.updated_at = Utc::now();

        server
    }
}
