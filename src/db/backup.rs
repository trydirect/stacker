use crate::models::{BackupDestination, BackupJob, BackupRun};
use sqlx::PgPool;

// ── Destinations ─────────────────────────────────────────────────────────

pub async fn insert_destination(
    pool: &PgPool,
    user_id: &str,
    name: &str,
    endpoint: &str,
    vault_path: &str,
) -> Result<BackupDestination, String> {
    sqlx::query_as::<_, BackupDestination>(
        r#"
        INSERT INTO backup_destination (user_id, name, endpoint, vault_path)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(name)
    .bind(endpoint)
    .bind(vault_path)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to insert backup destination: {}", e))
}

pub async fn fetch_destination(
    pool: &PgPool,
    id: i32,
) -> Result<Option<BackupDestination>, String> {
    sqlx::query_as::<_, BackupDestination>(r#"SELECT * FROM backup_destination WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to fetch backup destination: {}", e))
}

pub async fn list_destinations(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<BackupDestination>, String> {
    sqlx::query_as::<_, BackupDestination>(
        r#"SELECT * FROM backup_destination WHERE user_id = $1 ORDER BY id ASC"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list backup destinations: {}", e))
}

pub async fn delete_destination(pool: &PgPool, id: i32) -> Result<(), String> {
    sqlx::query(r#"DELETE FROM backup_destination WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| format!("Failed to delete backup destination: {}", e))
}

// ── Jobs ────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn insert_job(
    pool: &PgPool,
    user_id: &str,
    project_id: i32,
    destination_id: i32,
    name: &str,
    source: &str,
    target: &str,
    database: Option<&str>,
    schedule: &str,
    retention_daily: i32,
    retention_weekly: i32,
) -> Result<BackupJob, String> {
    sqlx::query_as::<_, BackupJob>(
        r#"
        INSERT INTO backup_job (
            user_id, project_id, destination_id, name, source, target,
            database, schedule, retention_daily, retention_weekly
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(project_id)
    .bind(destination_id)
    .bind(name)
    .bind(source)
    .bind(target)
    .bind(database)
    .bind(schedule)
    .bind(retention_daily)
    .bind(retention_weekly)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to insert backup job: {}", e))
}

pub async fn fetch_job(pool: &PgPool, id: i32) -> Result<Option<BackupJob>, String> {
    sqlx::query_as::<_, BackupJob>(r#"SELECT * FROM backup_job WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to fetch backup job: {}", e))
}

pub async fn list_jobs_by_project(
    pool: &PgPool,
    project_id: i32,
) -> Result<Vec<BackupJob>, String> {
    sqlx::query_as::<_, BackupJob>(
        r#"SELECT * FROM backup_job WHERE project_id = $1 ORDER BY id ASC"#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list backup jobs: {}", e))
}

/// Enabled jobs whose schedule is due (driven by the scheduler in WS-1 1.4).
pub async fn list_enabled_jobs(pool: &PgPool) -> Result<Vec<BackupJob>, String> {
    sqlx::query_as::<_, BackupJob>(r#"SELECT * FROM backup_job WHERE enabled ORDER BY id ASC"#)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Failed to list enabled backup jobs: {}", e))
}

pub async fn delete_job(pool: &PgPool, id: i32) -> Result<(), String> {
    sqlx::query(r#"DELETE FROM backup_job WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| format!("Failed to delete backup job: {}", e))
}

// ── Runs ────────────────────────────────────────────────────────────────

pub async fn insert_run(pool: &PgPool, job_id: i32, project_id: i32) -> Result<BackupRun, String> {
    sqlx::query_as::<_, BackupRun>(
        r#"
        INSERT INTO backup_run (job_id, project_id, status)
        VALUES ($1, $2, 'pending')
        RETURNING *
        "#,
    )
    .bind(job_id)
    .bind(project_id)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to insert backup run: {}", e))
}

pub async fn fetch_run(pool: &PgPool, id: i32) -> Result<Option<BackupRun>, String> {
    sqlx::query_as::<_, BackupRun>(r#"SELECT * FROM backup_run WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to fetch backup run: {}", e))
}

pub async fn list_runs_by_job(pool: &PgPool, job_id: i32) -> Result<Vec<BackupRun>, String> {
    sqlx::query_as::<_, BackupRun>(
        r#"SELECT * FROM backup_run WHERE job_id = $1 ORDER BY created_at DESC"#,
    )
    .bind(job_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list backup runs: {}", e))
}

/// Finished runs of a job (input to the retention policy in `backup-core`).
pub async fn list_finished_runs(pool: &PgPool, job_id: i32) -> Result<Vec<BackupRun>, String> {
    sqlx::query_as::<_, BackupRun>(
        r#"
        SELECT * FROM backup_run
        WHERE job_id = $1 AND status IN ('uploaded', 'restored')
        ORDER BY finished_at DESC NULLS LAST
        "#,
    )
    .bind(job_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list finished backup runs: {}", e))
}

pub async fn mark_run_running(pool: &PgPool, id: i32) -> Result<(), String> {
    sqlx::query(
        r#"
        UPDATE backup_run
        SET status = 'running', started_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to mark backup run running: {}", e))
}

pub async fn mark_run_uploaded(
    pool: &PgPool,
    id: i32,
    size_bytes: i64,
    checksum: &str,
    remote_file_id: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"
        UPDATE backup_run
        SET status = 'uploaded',
            size_bytes = $2,
            checksum = $3,
            remote_file_id = $4,
            finished_at = NOW(),
            error = NULL
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(size_bytes)
    .bind(checksum)
    .bind(remote_file_id)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to mark backup run uploaded: {}", e))
}

pub async fn mark_run_failed(pool: &PgPool, id: i32, error: &str) -> Result<(), String> {
    sqlx::query(
        r#"
        UPDATE backup_run
        SET status = 'failed', error = $2, finished_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(error)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to mark backup run failed: {}", e))
}

/// Mark runs pruned by retention (they are deleted from storage by the
/// connector; this keeps the row as an audit trail).
pub async fn mark_runs_pruned(pool: &PgPool, ids: &[i32]) -> Result<(), String> {
    sqlx::query(r#"UPDATE backup_run SET status = 'pruned' WHERE id = ANY($1)"#)
        .bind(ids)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| format!("Failed to mark backup runs pruned: {}", e))
}
