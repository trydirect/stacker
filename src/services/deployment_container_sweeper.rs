//! Housekeeping for retired container rows.
//!
//! `deployment_container` keeps a row after the app is removed rather than
//! deleting it on the spot, so the dashboard can show that a container went
//! away instead of quietly losing it. Those rows are of no interest a month
//! later, and a container that gets renamed leaves one behind that nothing will
//! ever revive.
//!
//! Deliberately modest in scope: it deletes only rows already marked
//! `removed_at`, so it cannot affect anything the dashboard displays. If it
//! never runs, the only consequence is a slowly growing table — bounded by the
//! containers a deployment has ever had.

use std::time::Duration;

use sqlx::PgPool;

use crate::db;

/// How often to sweep. A retired row lingering an extra hour costs nothing, so
/// there is no reason to run this more often than the rows appear.
const TICK: Duration = Duration::from_secs(3600);

/// How long a retired row is kept. Long enough that "the container disappeared
/// last week" is still answerable from the table.
const RETENTION_DAYS: i32 = 30;

pub fn spawn(pg_pool: PgPool) {
    tokio::spawn(async move {
        tracing::info!(
            "deployment_container_sweeper started (tick={:?}, retention={} days)",
            TICK,
            RETENTION_DAYS
        );
        loop {
            // Sleep first: startup is busy enough, and nothing here is urgent.
            tokio::time::sleep(TICK).await;

            match db::deployment_container::sweep_removed(&pg_pool, RETENTION_DAYS).await {
                Ok(0) => tracing::debug!("deployment_container_sweeper: nothing to remove"),
                Ok(count) => tracing::info!(
                    "deployment_container_sweeper: removed {} retired container row(s)",
                    count
                ),
                // A failed sweep is not worth escalating — the next tick tries
                // again, and stale rows are invisible to users either way.
                Err(err) => {
                    tracing::warn!("deployment_container_sweeper tick error: {}", err)
                }
            }
        }
    });
}
