//! Backup orchestration (WS-1): turn a job + destination into a typed agent
//! command and queue it for the target server's agent.
//!
//! The command payload carries an execution plan (process argv + upload/
//! download target) built by `backup-core` — no shell strings and no secrets.
//! The per-run bearer token is referenced only by environment-variable *name*
//! (`BACKUP_TOKEN`); its value is injected at execution time and never stored
//! in the command queue.

use crate::db;
use crate::models::{BackupDestination, BackupJob};
use crate::services::agent_dispatcher::AgentDispatcher;
use backup_core::{
    build_dump_plan, build_restore_plan, BackupSpec, DumpPlan, DumpSource, RestorePlan,
};
use serde_json::{json, Value};
use sqlx::PgPool;

/// Build the backup spec for a job + destination. Extra upload headers come
/// from the destination's Vault-held credentials (WS-1 1.3) and are empty
/// here.
pub fn spec_for(job: &BackupJob, destination: &BackupDestination) -> Result<BackupSpec, String> {
    let source = DumpSource::parse(&job.source)?;
    Ok(BackupSpec {
        source,
        target: job.target.clone(),
        database: job.database.clone().unwrap_or_default(),
        endpoint: destination.endpoint.clone(),
        extra_headers: Vec::new(),
    })
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

/// Build the dump plan for a job + destination (pure).
pub fn dump_plan_for(job: &BackupJob, destination: &BackupDestination) -> Result<DumpPlan, String> {
    let spec = spec_for(job, destination)?;
    build_dump_plan(&spec)
}

/// Build the restore plan for a job + destination and a stored file id.
pub fn restore_plan_for(
    job: &BackupJob,
    destination: &BackupDestination,
    remote_file_id: &str,
) -> Result<RestorePlan, String> {
    let spec = spec_for(job, destination)?;
    build_restore_plan(&spec, remote_file_id)
}

/// Create a pending run for `job` and queue the dump command to the agent on
/// `deployment_id`. Returns the run and the queued command id.
pub async fn enqueue_backup_run(
    pool: &PgPool,
    deployment_id: i32,
    job: &BackupJob,
    destination: &BackupDestination,
) -> Result<(i32, String), String> {
    let plan = dump_plan_for(job, destination)?;
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
) -> Result<String, String> {
    let plan = restore_plan_for(job, destination, remote_file_id)?;
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
        assert_eq!(plan.upload.auth_token_env, "BACKUP_TOKEN");
    }

    #[test]
    fn test_dump_command_params_reference_token_by_env_name_only() {
        let plan = dump_plan_for(
            &job("postgres", "shop-postgres-1", Some("shop")),
            &destination(),
        )
        .unwrap();
        let params = dump_command_params(42, &plan);
        assert_eq!(params["action"], "dump");
        assert_eq!(params["run_id"], 42);
        // The token value never appears; only the env var name is referenced.
        let serialized = params.to_string();
        assert!(!serialized.contains("Bearer"));
        assert!(serialized.contains("BACKUP_TOKEN"));
    }

    #[test]
    fn test_restore_plan_for_targets_file() {
        let plan = restore_plan_for(
            &job("postgres", "shop-postgres-1", Some("shop")),
            &destination(),
            "f-9",
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
        assert!(spec_for(&job("oracle", "x", None), &destination()).is_err());
    }
}
