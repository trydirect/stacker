//! Backup orchestration (WS-1): turn a job + destination into a typed agent
//! command and queue it for the target server's agent.
//!
//! The command payload carries an execution plan (process argv + upload/
//! download target) built by `backup-core`. The storage protocol is *data*
//! (`UploadTarget`), so the backend is pluggable. No secrets are stored: auth
//! and form values reference environment-variable names that the agent
//! resolves at run time.

use crate::db;
use crate::models::{BackupDestination, BackupJob};
use crate::services::agent_dispatcher::AgentDispatcher;
use backup_core::{
    build_dump_plan, build_restore_plan, Auth, BackupSpec, DownloadTarget, DumpPlan, DumpSource,
    FormField, RestorePlan, UploadBody, UploadTarget,
};
use serde_json::{json, Value};
use sqlx::PgPool;

/// What to dump (source/target/database from the job).
pub fn spec_for(job: &BackupJob) -> Result<BackupSpec, String> {
    let source = DumpSource::parse(&job.source)?;
    Ok(BackupSpec {
        source,
        target: job.target.clone(),
        database: job.database.clone().unwrap_or_default(),
    })
}

/// The upload protocol for a destination. The form/auth shape is data, so a
/// different backend (e.g. a restic or S3 target) swaps this builder without
/// touching the dump logic.
///
/// `token_url` is the OAuth2 token endpoint of the auth provider; the client
/// id/secret and the agent's device id are referenced by env-var name and
/// resolved by the agent at run time.
pub fn upload_target_for(destination: &BackupDestination, token_url: &str) -> UploadTarget {
    UploadTarget {
        url: format!("{}/upload", destination.endpoint.trim_end_matches('/')),
        body: UploadBody::Multipart {
            file_field: "file".to_string(),
            fields: vec![FormField {
                name: "device_id".to_string(),
                value: "$AGENT_ID".to_string(),
            }],
        },
        auth: Auth::ClientCredentials {
            token_url: token_url.to_string(),
            client_id_env: "BACKUP_CLIENT_ID".to_string(),
            client_secret_env: "BACKUP_CLIENT_SECRET".to_string(),
            scope: Some("openid".to_string()),
        },
        headers: Vec::new(),
    }
}

/// The download leg (restore) for a stored file.
pub fn download_target_for(
    destination: &BackupDestination,
    remote_file_id: &str,
    token_url: &str,
) -> DownloadTarget {
    DownloadTarget {
        url: format!(
            "{}/upload/files/{}",
            destination.endpoint.trim_end_matches('/'),
            remote_file_id
        ),
        auth: Auth::ClientCredentials {
            token_url: token_url.to_string(),
            client_id_env: "BACKUP_CLIENT_ID".to_string(),
            client_secret_env: "BACKUP_CLIENT_SECRET".to_string(),
            scope: Some("openid".to_string()),
        },
        headers: Vec::new(),
    }
}

/// Build the dump plan for a job + destination (pure).
pub fn dump_plan_for(
    job: &BackupJob,
    destination: &BackupDestination,
    token_url: &str,
) -> Result<DumpPlan, String> {
    let spec = spec_for(job)?;
    build_dump_plan(&spec, upload_target_for(destination, token_url))
}

/// Build the restore plan for a job + destination and a stored file id.
pub fn restore_plan_for(
    job: &BackupJob,
    destination: &BackupDestination,
    remote_file_id: &str,
    token_url: &str,
) -> Result<RestorePlan, String> {
    let spec = spec_for(job)?;
    build_restore_plan(
        &spec,
        download_target_for(destination, remote_file_id, token_url),
    )
}

/// `backup` agent command payload for a dump.
pub fn dump_command_params(run_id: i32, plan: &DumpPlan) -> Value {
    json!({
        "action": "dump",
        "run_id": run_id,
        "plan": plan,
    })
}

/// `backup` agent command payload for a restore.
pub fn restore_command_params(plan: &RestorePlan) -> Value {
    json!({
        "action": "restore",
        "plan": plan,
    })
}

/// Create a pending run for `job` and queue the dump command to the agent on
/// `deployment_id`. Returns the run id and the queued command id.
pub async fn enqueue_backup_run(
    pool: &PgPool,
    deployment_id: i32,
    job: &BackupJob,
    destination: &BackupDestination,
    token_url: &str,
) -> Result<(i32, String), String> {
    let plan = dump_plan_for(job, destination, token_url)?;
    let run = db::backup::insert_run(pool, job.id, job.project_id).await?;
    let params = dump_command_params(run.id, &plan);

    let dispatcher = AgentDispatcher::new(pool);
    let command_id = dispatcher
        .queue_command(deployment_id, "backup", params)
        .await
        .map_err(|e| format!("Failed to queue backup command: {}", e))?;

    Ok((run.id, command_id))
}

/// Queue a restore command for an existing run's stored file.
pub async fn enqueue_restore(
    pool: &PgPool,
    deployment_id: i32,
    job: &BackupJob,
    destination: &BackupDestination,
    remote_file_id: &str,
    token_url: &str,
) -> Result<String, String> {
    let plan = restore_plan_for(job, destination, remote_file_id, token_url)?;
    let params = restore_command_params(&plan);

    let dispatcher = AgentDispatcher::new(pool);
    dispatcher
        .queue_command(deployment_id, "backup", params)
        .await
        .map_err(|e| format!("Failed to queue restore command: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn job(source: &str, target: &str, database: Option<&str>) -> BackupJob {
        BackupJob {
            id: 7,
            user_id: "user-a".to_string(),
            project_id: 3,
            destination_id: 2,
            name: "nightly".to_string(),
            source: source.to_string(),
            target: target.to_string(),
            database: database.map(|d| d.to_string()),
            schedule: "0 3 * * *".to_string(),
            retention_daily: 7,
            retention_weekly: 4,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn destination() -> BackupDestination {
        BackupDestination {
            id: 2,
            user_id: "user-a".to_string(),
            name: "primary".to_string(),
            endpoint: "https://backup.example".to_string(),
            vault_path: "backup/primary".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_dump_plan_for_named_database_is_pure_argv() {
        let plan = dump_plan_for(
            &job("postgres", "shop-postgres-1", Some("shop")),
            &destination(),
            "https://login.example/token",
        )
        .unwrap();
        assert_eq!(
            plan.argv,
            vec![
                "docker",
                "exec",
                "shop-postgres-1",
                "pg_dump",
                "-Fc",
                "shop"
            ]
        );
        assert_eq!(plan.upload.url, "https://backup.example/upload");
    }

    #[test]
    fn test_upload_target_describes_multipart_and_client_credentials() {
        let upload = upload_target_for(&destination(), "https://login.example/token");
        match upload.body {
            UploadBody::Multipart { file_field, fields } => {
                assert_eq!(file_field, "file");
                assert_eq!(fields[0].name, "device_id");
                assert_eq!(fields[0].value, "$AGENT_ID");
            }
            _ => panic!("expected multipart"),
        }
        match upload.auth {
            Auth::ClientCredentials {
                token_url,
                client_secret_env,
                ..
            } => {
                assert_eq!(token_url, "https://login.example/token");
                assert_eq!(client_secret_env, "BACKUP_CLIENT_SECRET");
            }
            _ => panic!("expected client_credentials"),
        }
    }

    #[test]
    fn test_dump_command_params_reference_token_by_env_name_only() {
        let plan = dump_plan_for(
            &job("postgres", "shop-postgres-1", Some("shop")),
            &destination(),
            "https://login.example/token",
        )
        .unwrap();
        let params = dump_command_params(42, &plan);
        assert_eq!(params["action"], "dump");
        assert_eq!(params["run_id"], 42);
        // No secret value appears; only env-var *names* are referenced.
        let serialized = params.to_string();
        assert!(!serialized.contains("Bearer "));
        assert!(!serialized.contains("secret-value"));
        assert!(serialized.contains("BACKUP_CLIENT_SECRET"));
    }

    #[test]
    fn test_restore_plan_for_targets_file() {
        let plan = restore_plan_for(
            &job("postgres", "shop-postgres-1", Some("shop")),
            &destination(),
            "f-9",
            "https://login.example/token",
        )
        .unwrap();
        assert_eq!(plan.download.url, "https://backup.example/upload/files/f-9");
        assert_eq!(
            plan.argv,
            vec![
                "docker",
                "exec",
                "-i",
                "shop-postgres-1",
                "pg_restore",
                "--clean",
                "--if-exists",
                "-d",
                "shop"
            ]
        );
    }

    #[test]
    fn test_unknown_source_is_rejected() {
        assert!(spec_for(&job("oracle", "x", None)).is_err());
    }
}
