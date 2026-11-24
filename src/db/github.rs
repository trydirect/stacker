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

pub async fn fetch_repo(pool: &PgPool, id: i32) -> Result<Option<GithubRepo>, String> {
    sqlx::query_as::<_, GithubRepo>(
        r#"
        SELECT *
        FROM github_repo
        WHERE id = $1
        LIMIT 1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch github repo: {}", e))
}

/// Connected repos of one user, optionally narrowed to one project.
pub async fn list_repos(
    pool: &PgPool,
    user_id: &str,
    project_id: Option<i32>,
) -> Result<Vec<GithubRepo>, String> {
    sqlx::query_as::<_, GithubRepo>(
        r#"
        SELECT *
        FROM github_repo
        WHERE user_id = $1
          AND ($2::int IS NULL OR project_id = $2)
        ORDER BY id ASC
        "#,
    )
    .bind(user_id)
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list github repos: {}", e))
}

pub async fn update_repo(
    pool: &PgPool,
    id: i32,
    branch: Option<&str>,
    watch_paths: Option<&[String]>,
    autodeploy: Option<bool>,
) -> Result<GithubRepo, String> {
    sqlx::query_as::<_, GithubRepo>(
        r#"
        UPDATE github_repo
        SET branch = COALESCE($2, branch),
            watch_paths = COALESCE($3, watch_paths),
            autodeploy = COALESCE($4, autodeploy),
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(branch)
    .bind(watch_paths)
    .bind(autodeploy)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to update github repo: {}", e))
}

pub async fn delete_repo(pool: &PgPool, id: i32) -> Result<(), String> {
    sqlx::query(
        r#"
        DELETE FROM github_repo
        WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to delete github repo: {}", e))
}

/// Record (upsert) a GitHub App installation seen in a webhook.
/// `user_id` is filled by the Repo Connect flow (G4); webhook-only rows keep
/// the empty owner until then.
pub async fn upsert_installation(
    pool: &PgPool,
    installation_id: i64,
    account_login: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO github_installation (user_id, installation_id, account_login)
        VALUES ('', $1, $2)
        ON CONFLICT (user_id, installation_id) DO UPDATE
        SET account_login = COALESCE(EXCLUDED.account_login, github_installation.account_login),
            updated_at = NOW()
        "#,
    )
    .bind(installation_id)
    .bind(account_login)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to upsert github installation: {}", e))
}
