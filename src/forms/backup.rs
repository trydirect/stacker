use serde::{Deserialize, Serialize};

// ── Destinations ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDestinationRequest {
    pub name: String,
    pub endpoint: String,
    /// Vault path holding the device/service credentials for this destination.
    #[serde(rename = "vaultPath")]
    pub vault_path: String,
}

/// Never exposes `vault_path` or credentials — only a masked endpoint hint.
#[derive(Debug, Clone, Serialize)]
pub struct DestinationResponse {
    pub id: i32,
    pub name: String,
    /// Masked endpoint hint (host only).
    pub endpoint: String,
    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// ── Jobs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct CreateJobRequest {
    #[serde(rename = "projectId")]
    pub project_id: i32,
    #[serde(rename = "destinationId")]
    pub destination_id: i32,
    pub name: String,
    /// `postgres` | `mysql` | `mongo` | `volume`.
    pub source: String,
    /// Container name for database sources; volume name for `volume`.
    pub target: String,
    /// Database name; omitted uses the engine default.
    pub database: Option<String>,
    /// Cron expression. Defaults to daily 03:00.
    #[serde(default = "default_schedule")]
    pub schedule: String,
    #[serde(rename = "retentionDaily", default = "default_retention_daily")]
    pub retention_daily: i32,
    #[serde(rename = "retentionWeekly", default = "default_retention_weekly")]
    pub retention_weekly: i32,
}

fn default_schedule() -> String {
    "0 3 * * *".to_string()
}
fn default_retention_daily() -> i32 {
    7
}
fn default_retention_weekly() -> i32 {
    4
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateJobRequest {
    pub name: Option<String>,
    pub target: Option<String>,
    pub database: Option<String>,
    pub schedule: Option<String>,
    #[serde(rename = "retentionDaily")]
    pub retention_daily: Option<i32>,
    #[serde(rename = "retentionWeekly")]
    pub retention_weekly: Option<i32>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobResponse {
    pub id: i32,
    #[serde(rename = "projectId")]
    pub project_id: i32,
    #[serde(rename = "destinationId")]
    pub destination_id: i32,
    pub name: String,
    pub source: String,
    pub target: String,
    pub database: Option<String>,
    pub schedule: String,
    #[serde(rename = "retentionDaily")]
    pub retention_daily: i32,
    #[serde(rename = "retentionWeekly")]
    pub retention_weekly: i32,
    pub enabled: bool,
    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// ── Runs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RunResponse {
    pub id: i32,
    #[serde(rename = "jobId")]
    pub job_id: i32,
    #[serde(rename = "projectId")]
    pub project_id: i32,
    pub status: String,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: Option<i64>,
    pub checksum: Option<String>,
    #[serde(rename = "remoteFileId")]
    pub remote_file_id: Option<String>,
    pub error: Option<String>,
    #[serde(rename = "startedAt")]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(rename = "finishedAt")]
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Mask an endpoint to a host hint (drop path/credentials).
pub fn mask_endpoint(endpoint: &str) -> String {
    match url_host(endpoint) {
        Some(host) => host,
        None => "***".to_string(),
    }
}

fn url_host(endpoint: &str) -> Option<String> {
    let after_scheme = endpoint
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(endpoint);
    let host = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}
