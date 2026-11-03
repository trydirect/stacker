use crate::models::BakedSnapshot;
use sqlx::PgPool;

/// Which snapshots buyers may get, and in what order: a gated snapshot (baked
/// through the API) only through the `latest` or `stable` channel; `latest`
/// first, then `stable`, then the newest ungated one, so stacks baked before
/// channels existed keep working. `s` is `baked_snapshots`.
const SERVABLE: &str = r#"
    LEFT JOIN baked_snapshot_channels c
      ON c.snapshot_id = s.id AND c.channel IN ('latest', 'stable')
    WHERE s.healthy = TRUE
      AND (s.gated = FALSE OR c.channel IS NOT NULL)
"#;
const SERVE_ORDER: &str = r#"
    ORDER BY CASE c.channel WHEN 'latest' THEN 0 WHEN 'stable' THEN 1 ELSE 2 END,
             s.created_at DESC
    LIMIT 1
"#;

/// Resolve the snapshot buyers get for a stack+version on a provider. Used by
/// the clone deploy path to find what to clone from.
pub async fn resolve(
    pool: &PgPool,
    stack: &str,
    version: &str,
    provider: &str,
) -> Result<Option<BakedSnapshot>, String> {
    sqlx::query_as::<_, BakedSnapshot>(&format!(
        "SELECT s.* FROM baked_snapshots s {SERVABLE} \
         AND s.stack = $1 AND s.version = $2 AND s.provider = $3 {SERVE_ORDER}"
    ))
    .bind(stack)
    .bind(version)
    .bind(provider)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to resolve baked snapshot: {e}"))
}

/// Resolve the snapshot buyers get for a stack (any version): `latest`, else
/// `stable`, else the newest ungated healthy one.
pub async fn resolve_latest(
    pool: &PgPool,
    stack: &str,
    provider: &str,
) -> Result<Option<BakedSnapshot>, String> {
    sqlx::query_as::<_, BakedSnapshot>(&format!(
        "SELECT s.* FROM baked_snapshots s {SERVABLE} \
         AND s.stack = $1 AND s.provider = $2 {SERVE_ORDER}"
    ))
    .bind(stack)
    .bind(provider)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to resolve latest baked snapshot: {e}"))
}

/// The bake a stack's `unstable` channel points at: what the smoke clone checks
/// before promotion. `version`, when given, must be that bake's version; there
/// is no fallback to a released snapshot.
pub async fn resolve_unstable(
    pool: &PgPool,
    stack: &str,
    version: Option<&str>,
    provider: &str,
) -> Result<Option<BakedSnapshot>, String> {
    sqlx::query_as::<_, BakedSnapshot>(
        r#"
        SELECT s.* FROM baked_snapshot_channels c
        JOIN baked_snapshots s ON s.id = c.snapshot_id
        WHERE c.stack = $1 AND c.provider = $2 AND c.channel = 'unstable'
          AND s.healthy = TRUE AND ($3::TEXT IS NULL OR s.version = $3)
        "#,
    )
    .bind(stack)
    .bind(provider)
    .bind(version)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to resolve the unstable baked snapshot: {e}"))
}

/// Record a published bake. Returns the new row. The snapshot is served at
/// once (ungated): the `bake` binary's path.
#[allow(clippy::too_many_arguments)]
pub async fn record(
    pool: &PgPool,
    stack: &str,
    version: &str,
    provider: &str,
    image_id: i64,
    healthy: bool,
    digests: Option<serde_json::Value>,
    config_contract: Option<serde_json::Value>,
    required_env_keys: Option<serde_json::Value>,
    server_type: Option<&str>,
) -> Result<BakedSnapshot, String> {
    insert(
        pool,
        stack,
        version,
        provider,
        image_id,
        healthy,
        digests,
        config_contract,
        required_env_keys,
        server_type,
        false,
    )
    .await
}

/// Record a bake that must pass a smoke clone before buyers get it: the row is
/// gated and lands on the `unstable` channel, in one transaction.
#[allow(clippy::too_many_arguments)]
pub async fn record_unstable(
    pool: &PgPool,
    stack: &str,
    version: &str,
    provider: &str,
    image_id: i64,
    healthy: bool,
    digests: Option<serde_json::Value>,
    config_contract: Option<serde_json::Value>,
    required_env_keys: Option<serde_json::Value>,
    server_type: Option<&str>,
) -> Result<BakedSnapshot, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| format!("Failed to record baked snapshot: {e}"))?;
    let row = insert(
        &mut *tx,
        stack,
        version,
        provider,
        image_id,
        healthy,
        digests,
        config_contract,
        required_env_keys,
        server_type,
        true,
    )
    .await?;
    crate::db::bake_registry::point(
        &mut *tx,
        &row.stack,
        &row.provider,
        crate::helpers::bake_registry::CHANNEL_UNSTABLE,
        row.id,
    )
    .await?;
    tx.commit()
        .await
        .map_err(|e| format!("Failed to record baked snapshot: {e}"))?;
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
async fn insert<'e, E: sqlx::PgExecutor<'e>>(
    executor: E,
    stack: &str,
    version: &str,
    provider: &str,
    image_id: i64,
    healthy: bool,
    digests: Option<serde_json::Value>,
    config_contract: Option<serde_json::Value>,
    required_env_keys: Option<serde_json::Value>,
    server_type: Option<&str>,
    gated: bool,
) -> Result<BakedSnapshot, String> {
    sqlx::query_as::<_, BakedSnapshot>(
        r#"
        INSERT INTO baked_snapshots (stack, version, provider, image_id, healthy, digests, config_contract, required_env_keys, server_type, gated)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING *
        "#,
    )
    .bind(stack)
    .bind(version)
    .bind(provider)
    .bind(image_id)
    .bind(healthy)
    .bind(digests)
    .bind(config_contract)
    .bind(required_env_keys)
    .bind(server_type)
    .bind(gated)
    .fetch_one(executor)
    .await
    .map_err(|e| format!("Failed to record baked snapshot: {e}"))
}
