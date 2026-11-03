use crate::models::BakeJob;
use sqlx::PgPool;

pub struct NewBakeJob<'a> {
    pub stack: &'a str,
    pub version: &'a str,
    pub deployment_id: i32,
    pub server_id: i32,
    pub server_ip: &'a str,
    pub created_by: &'a str,
}

pub async fn insert(pool: &PgPool, job: NewBakeJob<'_>) -> Result<BakeJob, String> {
    sqlx::query_as::<_, BakeJob>(
        r#"
        INSERT INTO bake_job (stack, version, deployment_id, server_id, server_ip, created_by)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(job.stack)
    .bind(job.version)
    .bind(job.deployment_id)
    .bind(job.server_id)
    .bind(job.server_ip)
    .bind(job.created_by)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to record bake job: {e}"))
}

pub async fn fetch(pool: &PgPool, id: uuid::Uuid) -> Result<Option<BakeJob>, String> {
    sqlx::query_as::<_, BakeJob>("SELECT * FROM bake_job WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to read bake job: {e}"))
}

/// A bake already running for this build box. One at a time: two would
/// sanitize and snapshot the same box concurrently.
pub async fn running_for_server(pool: &PgPool, server_id: i32) -> Result<Option<BakeJob>, String> {
    sqlx::query_as::<_, BakeJob>(
        "SELECT * FROM bake_job WHERE server_id = $1 AND status = 'running' LIMIT 1",
    )
    .bind(server_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to read bake jobs: {e}"))
}

pub async fn mark_done(
    pool: &PgPool,
    id: uuid::Uuid,
    image_id: i64,
    snapshot_id: i32,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE bake_job SET status = 'done', image_id = $2, snapshot_id = $3, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(image_id)
    .bind(snapshot_id)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to finish bake job: {e}"))
}

pub async fn mark_failed(pool: &PgPool, id: uuid::Uuid, error: &str) -> Result<(), String> {
    sqlx::query(
        "UPDATE bake_job SET status = 'failed', error = $2, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(error)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to mark bake job failed: {e}"))
}
