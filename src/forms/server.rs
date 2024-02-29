use crate::models;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize, Debug, Validate)]
pub struct Server {
    pub id: i32,
    pub user_id: String,
    pub cloud_id: i32,
    pub project_id: i32,
    pub region: String,
    pub zone: Option<String>,
    pub server: String,
    pub os: String,
    pub disk_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Into<models::Server> for Server {
    fn into(self) -> models::Server {
        let mut server = models::Server::default();
        server.user_id = self.user_id;
        server.cloud_id = self.cloud_id;
        server.project_id = self.project_id;
        server.region = String::from("");
        server.zone = Some(String::from(""));
        server.server = String::from("");
        server.os = String::from("");
        server.created_at = Utc::now();
        server.updated_at = Utc::now();

        server
    }
}
