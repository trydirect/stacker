use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Agent {
    pub id: Uuid,
    pub deployment_hash: String,
    pub capabilities: Option<Value>,
    pub version: Option<String>,
    pub system_info: Option<Value>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Agent {
    pub fn new(deployment_hash: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            deployment_hash,
            capabilities: Some(serde_json::json!([])),
            version: None,
            system_info: Some(serde_json::json!({})),
            last_heartbeat: None,
            status: "offline".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub fn is_online(&self) -> bool {
        self.status == "online"
    }

    pub fn mark_online(&mut self) {
        self.status = "online".to_string();
        self.last_heartbeat = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn mark_offline(&mut self) {
        self.status = "offline".to_string();
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditLog {
    pub id: Uuid,
    pub agent_id: Option<Uuid>,
    pub deployment_hash: Option<String>,
    pub action: String,
    pub status: Option<String>,
    pub details: serde_json::Value,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl AuditLog {
    pub fn new(
        agent_id: Option<Uuid>,
        deployment_hash: Option<String>,
        action: String,
        status: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            agent_id,
            deployment_hash,
            action,
            status,
            details: serde_json::json!({}),
            ip_address: None,
            user_agent: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }

    pub fn with_ip(mut self, ip: String) -> Self {
        self.ip_address = Some(ip);
        self
    }

    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = Some(user_agent);
        self
    }
}
