use crate::models;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use chrono::{Utc};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Server {
    // pub cloud_id: i32,
    // pub project_id: i32,
    pub region: String,
    pub zone: Option<String>,
    pub server: String,
    pub os: String,
    pub disk_type: Option<String>,
}

impl Into<models::Server> for Server {
    fn into(self) -> models::Server {
        let mut server = models::Server::default();
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

impl Into<Server> for models::Server {

    fn into(self) -> Server {
        let mut form = Server::default();
        // form.cloud_id = self.cloud_id;
        // form.project_id = self.project_id;
        form.disk_type = self.disk_type;
        form.region = self.region;
        form.server = self.server;
        form.zone = self.zone;
        form.os = self.os;

        form
    }
}
