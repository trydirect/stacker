//! Housekeeping for dead and malformed agent rows.
//!
//! The `agents` table accumulates rows that no longer serve a purpose: agents
//! whose deployment was deleted or soft-deleted, agents that never sent a
//! heartbeat, and rows whose `deployment_hash` is structurally invalid (e.g. a
//! raw agent token stored instead of a hash). This sweeper removes them on a
//! daily cadence.
//!
//! A row is considered "dead" only when **both** of the following hold:
//!
//!   1. The deployment is gone — either no matching row in `deployment`, or the
//!      row exists with `deleted IS TRUE`.
//!   2. There is no sign of life within the retention window — neither a
//!      `last_heartbeat` nor an `audit_log` entry (the latter protects agents
//!      that are alive but failing authentication, since `auth_failure` is
//!      recorded in `audit_log` while `last_heartbeat` only advances on
//!      successful `wait`/`report`).
//!
//! Malformed rows (invalid `deployment_hash`) are deleted unconditionally,
//! without a retention window — such an agent can never authenticate by its own
//! hash.
//!
//! Agents whose deployment is alive but silent are **not** touched: the row is
//! the agent's identity, and removing it would require a full reinstall.

use std::time::Duration;

use sqlx::PgPool;

use crate::db;

/// How often to sweep. Rows appear rarely; daily is ample.
const TICK: Duration = Duration::from_secs(86_400);

/// How long a dead agent row is kept before removal. Long enough that a
/// temporarily stopped server can come back without losing its identity.
const RETENTION_DAYS: i32 = 30;

pub fn spawn(pg_pool: PgPool) {
    tokio::spawn(async move {
        tracing::info!(
            "agent_sweeper started (tick={:?}, retention={} days)",
            TICK,
            RETENTION_DAYS
        );
        loop {
            // Sleep first: startup is busy enough, and nothing here is urgent.
            tokio::time::sleep(TICK).await;

            // Malformed rows are independent of retention — log at warn because
            // new appearances indicate a write path that still needs fixing.
            match db::agent::sweep_malformed(&pg_pool).await {
                Ok(0) => {}
                Ok(count) => tracing::warn!(
                    "agent_sweeper: removed {} malformed agent row(s) — \
                     the write path producing these has not been found yet",
                    count
                ),
                Err(err) => {
                    tracing::warn!("agent_sweeper: malformed sweep error: {}", err)
                }
            }

            match db::agent::sweep_dead(&pg_pool, RETENTION_DAYS).await {
                Ok(0) => tracing::debug!("agent_sweeper: nothing to remove"),
                Ok(count) => tracing::info!(
                    "agent_sweeper: removed {} dead agent row(s)",
                    count
                ),
                Err(err) => {
                    tracing::warn!("agent_sweeper: dead sweep error: {}", err)
                }
            }
        }
    });
}
