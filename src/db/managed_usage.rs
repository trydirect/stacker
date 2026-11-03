//! Persistence of the Managed server usage behind a Quick Deploy.
//!
//! One row per deployment (`managed_usage`): the rate and cap it is billed
//! at, its lifecycle, and how far the daily report to the Stripe meter got.
//! The arithmetic lives in `services::managed_usage`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManagedUsage {
    pub id: i64,
    pub deployment_hash: String,
    pub user_id: String,
    pub stack: String,
    pub server_type: String,
    pub daily_rate_minor: i64,
    pub monthly_cap_minor: i64,
    pub currency: String,
    pub server_id: Option<i64>,
    pub status: String,
    pub outcome: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub suspended_at: Option<DateTime<Utc>>,
    pub period_start: Option<NaiveDate>,
    pub period_reported_minor: i64,
    pub last_reported_on: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ManagedUsage {
    /// The last day to report: today for a running server, the day it was
    /// suspended or deleted otherwise (never after today).
    pub fn report_until(&self, today: NaiveDate) -> NaiveDate {
        match self.status.as_str() {
            "active" => today,
            _ => self
                .suspended_at
                .or(self.stopped_at)
                .map(|at| at.date_naive().min(today))
                .unwrap_or(today),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewManagedUsage {
    pub deployment_hash: String,
    pub user_id: String,
    pub stack: String,
    pub server_type: String,
    pub daily_rate_minor: i64,
    pub monthly_cap_minor: i64,
}

pub async fn insert_pending(pool: &PgPool, new: &NewManagedUsage) -> Result<ManagedUsage, String> {
    sqlx::query_as!(
        ManagedUsage,
        r#"INSERT INTO managed_usage
               (deployment_hash, user_id, stack, server_type, daily_rate_minor, monthly_cap_minor)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING *"#,
        new.deployment_hash,
        new.user_id,
        new.stack,
        new.server_type,
        new.daily_rate_minor,
        new.monthly_cap_minor,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| format!("managed_usage insert: {e}"))
}

pub async fn fetch_by_hash(
    pool: &PgPool,
    deployment_hash: &str,
) -> Result<Option<ManagedUsage>, String> {
    sqlx::query_as!(
        ManagedUsage,
        "SELECT * FROM managed_usage WHERE deployment_hash = $1",
        deployment_hash,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("managed_usage fetch: {e}"))
}

/// The deploy succeeded: pending -> active. `None` when not pending.
pub async fn start(pool: &PgPool, deployment_hash: &str) -> Result<Option<ManagedUsage>, String> {
    sqlx::query_as!(
        ManagedUsage,
        r#"UPDATE managed_usage
           SET status = 'active', started_at = now(), updated_at = now()
           WHERE deployment_hash = $1 AND status = 'pending'
           RETURNING *"#,
        deployment_hash,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("managed_usage start: {e}"))
}

/// The deploy failed: pending -> closed with `outcome`. `None` when not pending.
pub async fn close(
    pool: &PgPool,
    deployment_hash: &str,
    outcome: &str,
) -> Result<Option<ManagedUsage>, String> {
    sqlx::query_as!(
        ManagedUsage,
        r#"UPDATE managed_usage
           SET status = 'closed', outcome = $2, stopped_at = now(), updated_at = now()
           WHERE deployment_hash = $1 AND status = 'pending'
           RETURNING *"#,
        deployment_hash,
        outcome,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("managed_usage close: {e}"))
}

/// The server was deleted: active or suspended -> stopped.
pub async fn stop(pool: &PgPool, deployment_hash: &str) -> Result<Option<ManagedUsage>, String> {
    sqlx::query_as!(
        ManagedUsage,
        r#"UPDATE managed_usage
           SET status = 'stopped', stopped_at = now(), updated_at = now()
           WHERE deployment_hash = $1 AND status IN ('active', 'suspended')
           RETURNING *"#,
        deployment_hash,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("managed_usage stop: {e}"))
}

/// An invoice is unpaid: every active server of the user -> suspended.
pub async fn suspend_for_user(pool: &PgPool, user_id: &str) -> Result<Vec<ManagedUsage>, String> {
    sqlx::query_as!(
        ManagedUsage,
        r#"UPDATE managed_usage
           SET status = 'suspended', suspended_at = now(), updated_at = now()
           WHERE user_id = $1 AND status = 'active'
           RETURNING *"#,
        user_id,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("managed_usage suspend: {e}"))
}

/// Rows with a day left to report up to `today`: running servers, and
/// stopped or suspended ones up to the day they stopped.
pub async fn list_due(
    pool: &PgPool,
    today: NaiveDate,
    limit: i64,
) -> Result<Vec<ManagedUsage>, String> {
    sqlx::query_as!(
        ManagedUsage,
        r#"SELECT * FROM managed_usage
           WHERE started_at IS NOT NULL
             AND (
                   (status = 'active'
                    AND (last_reported_on IS NULL OR last_reported_on < $1))
                OR (status IN ('stopped', 'suspended')
                    AND (last_reported_on IS NULL
                         OR last_reported_on < LEAST(
                                (COALESCE(suspended_at, stopped_at) AT TIME ZONE 'UTC')::date,
                                $1)))
             )
           ORDER BY last_reported_on ASC NULLS FIRST
           LIMIT $2"#,
        today,
        limit,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("managed_usage list_due: {e}"))
}

/// Store one reported day. `false` when that day was already recorded, so a
/// retry cannot count it twice.
pub async fn record_day(
    pool: &PgPool,
    id: i64,
    period_start: NaiveDate,
    period_reported_minor: i64,
    day: NaiveDate,
) -> Result<bool, String> {
    let result = sqlx::query!(
        r#"UPDATE managed_usage
           SET period_start = $2, period_reported_minor = $3, last_reported_on = $4,
               updated_at = now()
           WHERE id = $1 AND (last_reported_on IS NULL OR last_reported_on < $4)"#,
        id,
        period_start,
        period_reported_minor,
        day,
    )
    .execute(pool)
    .await
    .map_err(|e| format!("managed_usage record_day: {e}"))?;
    Ok(result.rows_affected() == 1)
}

pub async fn list_suspended_before(
    pool: &PgPool,
    cutoff: DateTime<Utc>,
) -> Result<Vec<ManagedUsage>, String> {
    sqlx::query_as!(
        ManagedUsage,
        r#"SELECT * FROM managed_usage
           WHERE status = 'suspended' AND suspended_at < $1
           ORDER BY suspended_at ASC"#,
        cutoff,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("managed_usage list_suspended_before: {e}"))
}

/// Keep the Hetzner server id: admins delete the VM by hand from the alert.
pub async fn set_server_id(
    pool: &PgPool,
    deployment_hash: &str,
    server_id: i64,
) -> Result<(), String> {
    sqlx::query!(
        "UPDATE managed_usage SET server_id = $2, updated_at = now() WHERE deployment_hash = $1",
        deployment_hash,
        server_id,
    )
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| format!("managed_usage set_server_id: {e}"))
}

/// Suspended for 3 days: closed and handed to admins to delete the VM by hand.
pub async fn mark_delete_due(
    pool: &PgPool,
    deployment_hash: &str,
) -> Result<Option<ManagedUsage>, String> {
    sqlx::query_as!(
        ManagedUsage,
        r#"UPDATE managed_usage
           SET status = 'closed', outcome = 'server_delete_due',
               stopped_at = now(), updated_at = now()
           WHERE deployment_hash = $1 AND status = 'suspended'
           RETURNING *"#,
        deployment_hash,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("managed_usage mark_delete_due: {e}"))
}
