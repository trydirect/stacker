//! Channel pointers for baked snapshots (`baked_snapshot_channels`). The
//! promotion decision itself is `helpers::bake_registry::promote_ladder`.

use crate::helpers::bake_registry::{
    promote_ladder, ChannelState, PromoteMoves, CHANNEL_LATEST, CHANNEL_STABLE, CHANNEL_UNSTABLE,
};
use sqlx::PgPool;

/// Point `channel` of a stack at a snapshot, replacing what it pointed at.
pub async fn point<'e, E: sqlx::PgExecutor<'e>>(
    executor: E,
    stack: &str,
    provider: &str,
    channel: &str,
    snapshot_id: i32,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO baked_snapshot_channels (stack, provider, channel, snapshot_id)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (stack, provider, channel)
        DO UPDATE SET snapshot_id = EXCLUDED.snapshot_id, updated_at = NOW()
        "#,
    )
    .bind(stack)
    .bind(provider)
    .bind(channel)
    .bind(snapshot_id)
    .execute(executor)
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to move the {channel} channel: {e}"))
}

/// Where each channel of a stack points now.
pub async fn state(pool: &PgPool, stack: &str, provider: &str) -> Result<ChannelState, String> {
    let rows: Vec<(String, i32)> = sqlx::query_as(
        "SELECT channel, snapshot_id FROM baked_snapshot_channels WHERE stack = $1 AND provider = $2",
    )
    .bind(stack)
    .bind(provider)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to read channels: {e}"))?;
    Ok(to_state(rows))
}

/// The channels of the stack (and provider) a snapshot belongs to.
pub async fn state_for_snapshot(pool: &PgPool, snapshot_id: i32) -> Result<ChannelState, String> {
    let rows: Vec<(String, i32)> = sqlx::query_as(
        r#"
        SELECT c.channel, c.snapshot_id
        FROM baked_snapshots s
        JOIN baked_snapshot_channels c ON c.stack = s.stack AND c.provider = s.provider
        WHERE s.id = $1
        "#,
    )
    .bind(snapshot_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to read channels: {e}"))?;
    Ok(to_state(rows))
}

fn to_state(rows: Vec<(String, i32)>) -> ChannelState {
    let mut state = ChannelState {
        unstable: None,
        latest: None,
        stable: None,
    };
    for (channel, id) in rows {
        match channel.as_str() {
            CHANNEL_UNSTABLE => state.unstable = Some(id),
            CHANNEL_LATEST => state.latest = Some(id),
            CHANNEL_STABLE => state.stable = Some(id),
            _ => {}
        }
    }
    state
}

#[derive(Debug)]
pub enum PromoteError {
    /// No such snapshot.
    NotFound,
    /// The snapshot is not what `unstable` points at: a newer bake replaced it,
    /// or it was never baked through the API.
    NotUnstable,
    Db(String),
}

impl std::fmt::Display for PromoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromoteError::NotFound => write!(f, "no such baked snapshot"),
            PromoteError::NotUnstable => write!(
                f,
                "the snapshot is not the stack's current unstable bake, so it cannot be promoted"
            ),
            PromoteError::Db(e) => write!(f, "{e}"),
        }
    }
}

/// Promote the stack's `unstable` snapshot to `latest` (old `latest` to
/// `stable`) after a green smoke clone. `snapshot_id` must be the current
/// `unstable`, so a smoke result is never applied to a different bake.
/// `Ok(None)` when it is already `latest`: repeating a promote is harmless.
pub async fn promote(
    pool: &PgPool,
    snapshot_id: i32,
) -> Result<Option<PromoteMoves>, PromoteError> {
    let db = |e: sqlx::Error| PromoteError::Db(format!("Failed to promote: {e}"));
    let mut tx = pool.begin().await.map_err(db)?;
    let (stack, provider): (String, String) =
        sqlx::query_as("SELECT stack, provider FROM baked_snapshots WHERE id = $1")
            .bind(snapshot_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or(PromoteError::NotFound)?;
    // Lock the stack's pointers: two promotes, or a bake landing meanwhile,
    // must not interleave.
    let rows: Vec<(String, i32)> = sqlx::query_as(
        "SELECT channel, snapshot_id FROM baked_snapshot_channels WHERE stack = $1 AND provider = $2 FOR UPDATE",
    )
    .bind(&stack)
    .bind(&provider)
    .fetch_all(&mut *tx)
    .await
    .map_err(db)?;
    let state = to_state(rows);
    if state.unstable != Some(snapshot_id) {
        return Err(PromoteError::NotUnstable);
    }
    let Some(moves) = promote_ladder(&state) else {
        return Ok(None);
    };
    point(
        &mut *tx,
        &stack,
        &provider,
        CHANNEL_LATEST,
        moves.new_latest,
    )
    .await
    .map_err(PromoteError::Db)?;
    if let Some(stable) = moves.new_stable {
        point(&mut *tx, &stack, &provider, CHANNEL_STABLE, stable)
            .await
            .map_err(PromoteError::Db)?;
    }
    tx.commit().await.map_err(db)?;
    Ok(Some(moves))
}
