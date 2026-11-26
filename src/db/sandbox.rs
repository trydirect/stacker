use crate::models::Sandbox;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn insert(
    pool: &PgPool,
    user_id: &str,
    project_id: Option<i32>,
    image: &str,
    runtime: &str,
    network_mode: &str,
    cpu: f64,
    mem_mb: i32,
    pids_limit: i32,
    inactivity_timeout_sec: i32,
    max_lifetime_sec: i32,
    env_encrypted: Option<&str>,
) -> Result<Sandbox, String> {
    sqlx::query_as::<_, Sandbox>(
        r#"
        INSERT INTO sandbox (
            id, user_id, project_id, image, runtime, network_mode,
            cpu, mem_mb, pids_limit, inactivity_timeout_sec, max_lifetime_sec,
            env_encrypted, expires_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9, $10, $11,
            $12, NOW() + make_interval(secs => $11)
        )
        RETURNING *
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(project_id)
    .bind(image)
    .bind(runtime)
    .bind(network_mode)
    .bind(cpu)
    .bind(mem_mb)
    .bind(pids_limit)
    .bind(inactivity_timeout_sec)
    .bind(max_lifetime_sec)
    .bind(env_encrypted)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to insert sandbox: {}", e))
}

pub async fn fetch(pool: &PgPool, id: Uuid) -> Result<Option<Sandbox>, String> {
    sqlx::query_as::<_, Sandbox>(r#"SELECT * FROM sandbox WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to fetch sandbox: {}", e))
}

pub async fn list_by_user(pool: &PgPool, user_id: &str) -> Result<Vec<Sandbox>, String> {
    sqlx::query_as::<_, Sandbox>(
        r#"SELECT * FROM sandbox WHERE user_id = $1 ORDER BY created_at DESC"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list sandboxes: {}", e))
}

pub async fn update_status(pool: &PgPool, id: Uuid, status: &str) -> Result<(), String> {
    sqlx::query(r#"UPDATE sandbox SET status = $2, updated_at = NOW() WHERE id = $1"#)
        .bind(id)
        .bind(status)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| format!("Failed to update sandbox status: {}", e))
}

/// Sandboxes past their hard lifetime (or a caller-supplied cutoff) that are
/// still alive — input to the lifecycle manager's kill pass.
pub async fn list_expired(
    pool: &PgPool,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<Sandbox>, String> {
    sqlx::query_as::<_, Sandbox>(
        r#"
        SELECT * FROM sandbox
        WHERE status IN ('creating', 'running') AND expires_at <= $1
        ORDER BY expires_at ASC
        "#,
    )
    .bind(now)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list expired sandboxes: {}", e))
}

/// Delete a sandbox row (after its container is gone).
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), String> {
    sqlx::query(r#"DELETE FROM sandbox WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| format!("Failed to delete sandbox: {}", e))
}
