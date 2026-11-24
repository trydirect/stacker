use crate::models::GithubRepo;
use sqlx::PgPool;

/// Record an incoming webhook delivery. Returns `false` when the delivery id
/// was already processed (replay protection, WS-G G2).
pub async fn record_delivery(
    pool: &PgPool,
    delivery_id: &str,
    event_type: &str,
) -> Result<bool, String> {
    sqlx::query(
        r#"
        INSERT INTO github_webhook_delivery (delivery_id, event_type)
        VALUES ($1, $2)
        ON CONFLICT (delivery_id) DO NOTHING
        "#,
    )
    .bind(delivery_id)
    .bind(event_type)
    .execute(pool)
    .await
    .map(|result| result.rows_affected() > 0)
    .map_err(|e| format!("Failed to record github webhook delivery: {}", e))
}

/// Autodeploy-enabled repos bound to a pushed branch (WS-G G5 input).
pub async fn list_push_repos(
    pool: &PgPool,
    repo: &str,
    branch: &str,
) -> Result<Vec<GithubRepo>, String> {
    sqlx::query_as::<_, GithubRepo>(
        r#"
        SELECT *
        FROM github_repo
        WHERE repo = $1
          AND branch = $2
          AND autodeploy
        ORDER BY id ASC
        "#,
    )
    .bind(repo)
    .bind(branch)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list github repos for push: {}", e))
}

pub async fn insert_repo(
    pool: &PgPool,
    user_id: &str,
    installation_id: i64,
    project_id: Option<i32>,
    repo: &str,
    branch: &str,
    watch_paths: &[String],
    autodeploy: bool,
) -> Result<GithubRepo, String> {
    sqlx::query_as::<_, GithubRepo>(
        r#"
        INSERT INTO github_repo
            (user_id, installation_id, project_id, repo, branch, watch_paths, autodeploy)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(installation_id)
    .bind(project_id)
    .bind(repo)
    .bind(branch)
    .bind(watch_paths)
    .bind(autodeploy)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to insert github repo: {}", e))
}
