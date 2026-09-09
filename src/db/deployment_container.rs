//! Reads and writes of the observed-container table.
//!
//! The table is what makes the dashboard's container list stop moving on its
//! own. See `migrations/20260909140000_deployment_container.up.sql` for why it
//! exists and `models::DeploymentContainer` for the shape.

use crate::models::{DeploymentContainer, ObservedContainer};
use sqlx::PgPool;
use tracing::Instrument;

/// Record what an aggregate health report saw.
///
/// One statement per container. A batch would be tidier, but reports carry a
/// handful of containers and the clarity is worth more than the round trips
/// here — this runs once per report, not per request.
///
/// **Only aggregate reports may call this.** A single-app report describes one
/// container and knows nothing about the rest; treating it as an observation of
/// the deployment is what put the Status Panel in the user's own container list
/// (see `routes::agent::snapshot::assemble_containers`).
///
/// An observation also revives a container that was marked removed: if it is
/// running again, whatever retired it was wrong or the app was reinstalled.
#[tracing::instrument(name = "Record observed containers", skip(pool, containers), fields(count = containers.len()))]
pub async fn record_observed(
    pool: &PgPool,
    deployment_hash: &str,
    containers: &[ObservedContainer],
) -> Result<(), String> {
    for container in containers {
        let query_span = tracing::info_span!("Upsert observed container");
        sqlx::query!(
            r#"
            INSERT INTO deployment_container (
                deployment_hash, container_name, app_code, scope, image, state,
                first_seen_at, last_seen_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
            ON CONFLICT (deployment_hash, container_name) DO UPDATE SET
                app_code = COALESCE(EXCLUDED.app_code, deployment_container.app_code),
                scope = EXCLUDED.scope,
                image = COALESCE(EXCLUDED.image, deployment_container.image),
                state = EXCLUDED.state,
                last_seen_at = NOW(),
                removed_at = NULL
            "#,
            deployment_hash,
            container.container_name,
            container.app_code,
            container.scope,
            container.image,
            container.state,
        )
        .execute(pool)
        .instrument(query_span)
        .await
        .map_err(|err| {
            tracing::error!("Failed to record observed container: {:?}", err);
            format!("Database error: {}", err)
        })?;
    }

    Ok(())
}

/// Every container of a deployment that has not been deliberately removed.
///
/// Ordered by name so the list a user sees does not shuffle between requests —
/// stable membership is worth little if the order moves instead.
#[tracing::instrument(name = "Fetch deployment containers", skip(pool))]
pub async fn fetch_by_deployment(
    pool: &PgPool,
    deployment_hash: &str,
) -> Result<Vec<DeploymentContainer>, String> {
    sqlx::query_as!(
        DeploymentContainer,
        r#"
        SELECT id, deployment_hash, container_name, app_code, scope, image,
               state, first_seen_at, last_seen_at, removed_at
        FROM deployment_container
        WHERE deployment_hash = $1
          AND removed_at IS NULL
        ORDER BY container_name ASC
        "#,
        deployment_hash,
    )
    .fetch_all(pool)
    .await
    .map_err(|err| {
        tracing::error!("Failed to fetch deployment containers: {:?}", err);
        format!("Database error: {}", err)
    })
}

/// Retire every container belonging to one app.
///
/// Called when `remove_app` completes — the only thing that takes a container
/// off the list. Rows are marked, not deleted, so the dashboard can say a
/// container went away instead of quietly losing it; a sweeper clears them
/// later.
#[tracing::instrument(name = "Mark deployment containers removed", skip(pool))]
pub async fn mark_removed_by_app_code(
    pool: &PgPool,
    deployment_hash: &str,
    app_code: &str,
) -> Result<u64, String> {
    let result = sqlx::query!(
        r#"
        UPDATE deployment_container
        SET removed_at = NOW()
        WHERE deployment_hash = $1
          AND app_code = $2
          AND removed_at IS NULL
        "#,
        deployment_hash,
        app_code,
    )
    .execute(pool)
    .await
    .map_err(|err| {
        tracing::error!("Failed to mark containers removed: {:?}", err);
        format!("Database error: {}", err)
    })?;

    Ok(result.rows_affected())
}

/// Delete rows retired longer ago than `older_than_days`.
///
/// Housekeeping rather than necessity: the row count is bounded by containers
/// per deployment. It keeps a renamed container's orphaned row from lingering
/// for ever.
#[tracing::instrument(name = "Sweep removed deployment containers", skip(pool))]
pub async fn sweep_removed(pool: &PgPool, older_than_days: i32) -> Result<u64, String> {
    let result = sqlx::query!(
        r#"
        DELETE FROM deployment_container
        WHERE removed_at IS NOT NULL
          AND removed_at < NOW() - make_interval(days => $1)
        "#,
        older_than_days,
    )
    .execute(pool)
    .await
    .map_err(|err| {
        tracing::error!("Failed to sweep removed containers: {:?}", err);
        format!("Database error: {}", err)
    })?;

    Ok(result.rows_affected())
}
