use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A short-lived sandbox container for untrusted / AI-generated code (WS-S).
/// `env_encrypted` is write-only: encrypted at rest, never serialized out.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Sandbox {
    pub id: uuid::Uuid,
    pub user_id: String,
    pub project_id: Option<i32>,
    /// Where it runs (the agent addressed for exec/kill).
    pub deployment_id: Option<i32>,
    /// `creating` | `running` | `killed` | `failed` | `expired`.
    pub status: String,
    pub image: String,
    /// `runc` | `runsc` (gVisor).
    pub runtime: String,
    /// `isolated` | `none`.
    pub network_mode: String,
    pub cpu: f64,
    pub mem_mb: i32,
    pub pids_limit: i32,
    pub inactivity_timeout_sec: i32,
    pub max_lifetime_sec: i32,
    /// Encrypted env (write-only, never returned by the API).
    pub env_encrypted: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// The API view of a sandbox — deliberately omits `env_encrypted`.
#[derive(Debug, Clone, Serialize)]
pub struct SandboxResponse {
    pub id: uuid::Uuid,
    #[serde(rename = "projectId")]
    pub project_id: Option<i32>,
    pub status: String,
    pub image: String,
    pub runtime: String,
    #[serde(rename = "networkMode")]
    pub network_mode: String,
    pub cpu: f64,
    #[serde(rename = "memMb")]
    pub mem_mb: i32,
    #[serde(rename = "pidsLimit")]
    pub pids_limit: i32,
    #[serde(rename = "inactivityTimeoutSec")]
    pub inactivity_timeout_sec: i32,
    #[serde(rename = "maxLifetimeSec")]
    pub max_lifetime_sec: i32,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "expiresAt")]
    pub expires_at: DateTime<Utc>,
}

impl From<&Sandbox> for SandboxResponse {
    fn from(s: &Sandbox) -> Self {
        Self {
            id: s.id,
            project_id: s.project_id,
            status: s.status.clone(),
            image: s.image.clone(),
            runtime: s.runtime.clone(),
            network_mode: s.network_mode.clone(),
            cpu: s.cpu,
            mem_mb: s.mem_mb,
            pids_limit: s.pids_limit,
            inactivity_timeout_sec: s.inactivity_timeout_sec,
            max_lifetime_sec: s.max_lifetime_sec,
            created_at: s.created_at,
            expires_at: s.expires_at,
        }
    }
}

/// Sandbox creation request. `env` is write-only (encrypted before storage).
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSandboxRequest {
    #[serde(rename = "projectId")]
    pub project_id: Option<i32>,
    pub image: String,
    #[serde(default = "default_runtime")]
    pub runtime: String,
    #[serde(default = "default_network_mode")]
    pub network_mode: String,
    #[serde(default = "default_cpu")]
    pub cpu: f64,
    #[serde(rename = "memMb", default = "default_mem_mb")]
    pub mem_mb: i32,
    #[serde(rename = "pidsLimit", default = "default_pids_limit")]
    pub pids_limit: i32,
    #[serde(
        rename = "inactivityTimeoutSec",
        default = "default_inactivity_timeout"
    )]
    pub inactivity_timeout_sec: i32,
    #[serde(rename = "maxLifetimeSec", default = "default_max_lifetime")]
    pub max_lifetime_sec: i32,
    /// Write-only env; encrypted at rest, never returned.
    #[serde(default)]
    pub env: Option<serde_json::Value>,
}

fn default_runtime() -> String {
    "runc".to_string()
}
fn default_network_mode() -> String {
    "isolated".to_string()
}
fn default_cpu() -> f64 {
    1.0
}
fn default_mem_mb() -> i32 {
    512
}
fn default_pids_limit() -> i32 {
    128
}
fn default_inactivity_timeout() -> i32 {
    1800
}
fn default_max_lifetime() -> i32 {
    86400
}
