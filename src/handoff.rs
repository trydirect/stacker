use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Shared server/CLI contract for resuming management of an existing deployment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentHandoffPayload {
    pub version: String,
    pub expires_at: DateTime<Utc>,
    pub project: DeploymentHandoffProject,
    pub deployment: DeploymentHandoffDeployment,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<DeploymentHandoffServerContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<DeploymentHandoffCloudContext>,
    pub lockfile: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stacker_yml: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<DeploymentHandoffAgentContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentHandoffProject {
    pub id: i32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentHandoffDeployment {
    pub id: i32,
    pub hash: String,
    pub target: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DeploymentHandoffServerContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DeploymentHandoffCloudContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DeploymentHandoffAgentContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentHandoffLink {
    pub url: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl DeploymentHandoffLink {
    pub fn new(url: String, issued_at: DateTime<Utc>, expires_at: DateTime<Utc>) -> Self {
        Self {
            url,
            issued_at,
            expires_at,
        }
    }

    pub fn is_expired_at(&self, reference_time: DateTime<Utc>) -> bool {
        reference_time >= self.expires_at
    }
}
