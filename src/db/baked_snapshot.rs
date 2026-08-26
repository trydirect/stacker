use crate::models::BakedSnapshot;
use sqlx::PgPool;

/// Resolve the most recent healthy baked snapshot image_id for a stack+version
/// on a provider. Used by the clone deploy path to find what to clone from.
pub async fn resolve(
    pool: &PgPool,
    stack: &str,
    version: &str,
    provider: &str,
) -> Result<Option<BakedSnapshot>, String> {
    sqlx::query_as::<_, BakedSnapshot>(
        r#"
        SELECT *
        FROM baked_snapshots
        WHERE stack = $1
          AND version = $2
          AND provider = $3
          AND healthy = TRUE
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(stack)
    .bind(version)
    .bind(provider)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to resolve baked snapshot: {e}"))
}

/// Resolve any healthy snapshot for a stack (any version), newest first.
/// Used when the exact version has not been baked but a generic one exists.
pub async fn resolve_latest(
    pool: &PgPool,
    stack: &str,
    provider: &str,
) -> Result<Option<BakedSnapshot>, String> {
    sqlx::query_as::<_, BakedSnapshot>(
        r#"
        SELECT *
        FROM baked_snapshots
        WHERE stack = $1
          AND provider = $2
          AND healthy = TRUE
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(stack)
    .bind(provider)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to resolve latest baked snapshot: {e}"))
}

/// Record a published bake. Returns the new row.
pub async fn record(
    pool: &PgPool,
    stack: &str,
    version: &str,
    provider: &str,
    image_id: i64,
    healthy: bool,
    digests: Option<serde_json::Value>,
) -> Result<BakedSnapshot, String> {
    sqlx::query_as::<_, BakedSnapshot>(
        r#"
        INSERT INTO baked_snapshots (stack, version, provider, image_id, healthy, digests)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(stack)
    .bind(version)
    .bind(provider)
    .bind(image_id)
    .bind(healthy)
    .bind(digests)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to record baked snapshot: {e}"))
}
