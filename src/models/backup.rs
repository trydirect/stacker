use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Where backups are stored. Credentials (device secret, per-run token) live
/// only in Vault under `vault_path` — never here, never serialized out.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BackupDestination {
    pub id: i32,
    pub user_id: String,
    pub name: String,
    /// Upload endpoint of the storage backend (masked in API responses).
    pub endpoint: String,
    /// Vault path holding the device credentials for this destination.
    pub vault_path: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A scheduled backup: what to dump, where to send it, and how long to keep it.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BackupJob {
    pub id: i32,
    pub user_id: String,
    pub project_id: i32,
    pub destination_id: i32,
    pub name: String,
    /// Dump source: `postgres` | `mysql` | `mongo` | `volume`.
    pub source: String,
    /// Container name for database sources; volume name for `volume`.
    pub target: String,
    /// Database name (postgres/mysql); empty uses the engine default.
    pub database: Option<String>,
    /// Cron expression for the schedule.
    pub schedule: String,
    pub retention_daily: i32,
    pub retention_weekly: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One backup attempt for a job.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BackupRun {
    pub id: i32,
    pub job_id: i32,
    pub project_id: i32,
    /// `pending` | `running` | `uploaded` | `failed` | `pruned` | `restored`.
    pub status: String,
    pub size_bytes: Option<i64>,
    pub checksum: Option<String>,
    /// File id in the storage backend (used for restore and retention).
    pub remote_file_id: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
