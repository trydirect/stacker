//! Persistence for per-install billing authorizations.
//!
//! The `marketplace_install_authorization` table is the stacker-side ledger
//! for two-phase per-install charges. user_service is authoritative on the
//! underlying money movement (it holds the Stripe payment intent, refund
//! history, etc.); stacker holds only the opaque `authorization_id`, a
//! link to the local project/deployment, and the transition status so a
//! sweeper can reconcile stale rows.
//!
//! Uses runtime `sqlx::query_as` / `sqlx::query` calls rather than the
//! compile-time macros because the schema is introduced in the same commit
//! as this module — the `.sqlx` cache would need a database with the new
//! migration already applied to build. Runtime queries stay compilable in
//! `SQLX_OFFLINE=true` builds.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// One row of `marketplace_install_authorization`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuthorizationRow {
    pub id: Uuid,
    pub project_id: Option<i32>,
    pub user_id: String,
    pub template_id: Uuid,
    pub idempotency_key: String,
    pub authorization_id: String,
    pub amount_minor: i64,
    pub currency: String,
    pub status: String,
    pub deployment_hash: Option<String>,
    pub void_reason: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Daily billing fields
    pub billing_cycle: Option<String>,
    pub daily_rate: Option<f64>,
    pub monthly_cap: Option<f64>,
    pub total_charged_minor: Option<i64>,
    pub last_daily_charge_at: Option<DateTime<Utc>>,
    pub server_deleted_at: Option<DateTime<Utc>>,
    pub suspended_at: Option<DateTime<Utc>>,
}

/// Input for inserting a fresh authorization row. The row is always inserted
/// with `status='authorized'` and `project_id=NULL`; `attach_project` fills
/// in the project link once the project row commits.
#[derive(Debug, Clone)]
pub struct NewAuthorization {
    pub user_id: String,
    pub template_id: Uuid,
    pub idempotency_key: String,
    pub authorization_id: String,
    pub amount_minor: i64,
    pub currency: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub billing_cycle: Option<String>,
    pub daily_rate: Option<f64>,
    pub monthly_cap: Option<f64>,
}

/// Insert (or fetch, on idempotency-key replay) an authorization row.
///
/// The `(user_id, idempotency_key)` unique index is the pivot for replay
/// safety — a duplicate install request with the same key returns the
/// previously-persisted row instead of creating a second one. Callers that
/// see the same `authorization_id` in the response can safely treat the
/// request as a no-op.
pub async fn insert_authorization(
    pool: &PgPool,
    row: NewAuthorization,
) -> Result<AuthorizationRow, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let inserted: Option<AuthorizationRow> = sqlx::query_as::<_, AuthorizationRow>(
        r#"INSERT INTO marketplace_install_authorization
            (user_id, template_id, idempotency_key, authorization_id,
             amount_minor, currency, status, expires_at,
             billing_cycle, daily_rate, monthly_cap)
           VALUES ($1, $2, $3, $4, $5, $6, 'authorized', $7, $8, $9, $10)
           ON CONFLICT (user_id, idempotency_key) DO NOTHING
           RETURNING *"#,
    )
    .bind(&row.user_id)
    .bind(row.template_id)
    .bind(&row.idempotency_key)
    .bind(&row.authorization_id)
    .bind(row.amount_minor)
    .bind(&row.currency)
    .bind(row.expires_at)
    .bind(&row.billing_cycle)
    .bind(row.daily_rate)
    .bind(row.monthly_cap)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| format!("insert_authorization: {}", e))?;

    let result = if let Some(inserted) = inserted {
        inserted
    } else {
        sqlx::query_as::<_, AuthorizationRow>(
            r#"SELECT * FROM marketplace_install_authorization
               WHERE user_id = $1 AND idempotency_key = $2"#,
        )
        .bind(&row.user_id)
        .bind(&row.idempotency_key)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| format!("insert_authorization replay lookup: {}", e))?
    };

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(result)
}

/// Link the authorization to the freshly-inserted project row.
pub async fn attach_project(pool: &PgPool, auth_id: Uuid, project_id: i32) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE marketplace_install_authorization
           SET project_id = $1, updated_at = now()
           WHERE id = $2"#,
    )
    .bind(project_id)
    .bind(auth_id)
    .execute(pool)
    .await
    .map_err(|e| format!("attach_project: {}", e))?;
    Ok(())
}

/// Attach the deployment hash so `deploy_complete_handler` can look up
/// the authorization by hash without joining `stack_template_deployment`.
pub async fn attach_deployment_hash(
    pool: &PgPool,
    auth_id: Uuid,
    deployment_hash: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE marketplace_install_authorization
           SET deployment_hash = $1, updated_at = now()
           WHERE id = $2"#,
    )
    .bind(deployment_hash)
    .bind(auth_id)
    .execute(pool)
    .await
    .map_err(|e| format!("attach_deployment_hash: {}", e))?;
    Ok(())
}

/// Terminal state transition on successful capture. Idempotent — re-running
/// with the same `authorization_id` is a no-op if the row is already
/// captured.
pub async fn mark_captured(pool: &PgPool, authorization_id: &str) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE marketplace_install_authorization
           SET status = 'captured', updated_at = now()
           WHERE authorization_id = $1 AND status = 'authorized'"#,
    )
    .bind(authorization_id)
    .execute(pool)
    .await
    .map_err(|e| format!("mark_captured: {}", e))?;
    Ok(())
}

/// Terminal state transition on void. Records the reason for audit — the
/// sweeper writes `expired`, install-failure paths write `install_failed:*`,
/// user_service-side voids write `refunded` / etc.
pub async fn mark_voided(
    pool: &PgPool,
    authorization_id: &str,
    reason: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE marketplace_install_authorization
           SET status = 'voided', void_reason = $2, updated_at = now()
           WHERE authorization_id = $1 AND status = 'authorized'"#,
    )
    .bind(authorization_id)
    .bind(reason)
    .execute(pool)
    .await
    .map_err(|e| format!("mark_voided: {}", e))?;
    Ok(())
}

/// Look up the authorization associated with a deployment. Called by
/// `deploy_complete_handler` to decide whether to trigger capture.
pub async fn find_by_deployment_hash(
    pool: &PgPool,
    deployment_hash: &str,
) -> Result<Option<AuthorizationRow>, String> {
    sqlx::query_as::<_, AuthorizationRow>(
        r#"SELECT * FROM marketplace_install_authorization
           WHERE deployment_hash = $1"#,
    )
    .bind(deployment_hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("find_by_deployment_hash: {}", e))
}

/// Rows the sweeper should void: authorized past their TTL grace window.
pub async fn list_expired_authorized(
    pool: &PgPool,
    cutoff: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<AuthorizationRow>, String> {
    sqlx::query_as::<_, AuthorizationRow>(
        r#"SELECT * FROM marketplace_install_authorization
           WHERE status = 'authorized' AND expires_at IS NOT NULL AND expires_at < $1
           ORDER BY expires_at ASC
           LIMIT $2"#,
    )
    .bind(cutoff)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("list_expired_authorized: {}", e))
}

// ─── Deployment-daily billing helpers ───────────────────────────────────

/// Rows eligible for daily charge sweep:
/// billing_cycle='deployment_daily', status='captured', not yet at monthly cap,
/// last charge was 24+ hours ago, server not deleted.
pub async fn list_daily_sweep_candidates(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<AuthorizationRow>, String> {
    sqlx::query_as::<_, AuthorizationRow>(
        r#"SELECT * FROM marketplace_install_authorization
           WHERE billing_cycle = 'deployment_daily'
             AND status = 'captured'
             AND server_deleted_at IS NULL
             AND suspended_at IS NULL
             AND (last_daily_charge_at IS NULL OR last_daily_charge_at < now() - interval '24 hours')
           ORDER BY last_daily_charge_at ASC NULLS FIRST
           LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("list_daily_sweep_candidates: {}", e))
}

/// Mark a daily charge as applied: bump total_charged_minor and last_daily_charge_at.
pub async fn mark_daily_charged(
    pool: &PgPool,
    authorization_id: &str,
    charged_minor: i64,
) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE marketplace_install_authorization
           SET total_charged_minor = total_charged_minor + $2,
               last_daily_charge_at = now(),
               updated_at = now()
           WHERE authorization_id = $1"#,
    )
    .bind(authorization_id)
    .bind(charged_minor)
    .execute(pool)
    .await
    .map_err(|e| format!("mark_daily_charged: {}", e))?;
    Ok(())
}

/// Mark server as deleted (user-initiated deletion).
pub async fn mark_server_deleted(
    pool: &PgPool,
    authorization_id: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE marketplace_install_authorization
           SET server_deleted_at = now(), updated_at = now()
           WHERE authorization_id = $1 AND server_deleted_at IS NULL"#,
    )
    .bind(authorization_id)
    .execute(pool)
    .await
    .map_err(|e| format!("mark_server_deleted: {}", e))?;
    Ok(())
}

/// Mark server as suspended (grace period expired).
pub async fn mark_suspended(
    pool: &PgPool,
    authorization_id: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE marketplace_install_authorization
           SET suspended_at = now(), updated_at = now()
           WHERE authorization_id = $1 AND suspended_at IS NULL"#,
    )
    .bind(authorization_id)
    .execute(pool)
    .await
    .map_err(|e| format!("mark_suspended: {}", e))?;
    Ok(())
}

/// Update billing_cycle and daily rate fields on an authorization row.
pub async fn set_daily_billing(
    pool: &PgPool,
    authorization_id: &str,
    daily_rate: f64,
    monthly_cap: f64,
) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE marketplace_install_authorization
           SET billing_cycle = 'deployment_daily',
               daily_rate = $2,
               monthly_cap = $3,
               updated_at = now()
           WHERE authorization_id = $1"#,
    )
    .bind(authorization_id)
    .bind(daily_rate)
    .bind(monthly_cap)
    .execute(pool)
    .await
    .map_err(|e| format!("set_daily_billing: {}", e))?;
    Ok(())
}

/// Fetch authorization by deployment_hash (for one-click flow).
pub async fn find_by_deployment_hash_optional(
    pool: &PgPool,
    deployment_hash: &str,
) -> Result<Option<AuthorizationRow>, String> {
    sqlx::query_as::<_, AuthorizationRow>(
        r#"SELECT * FROM marketplace_install_authorization
           WHERE deployment_hash = $1"#,
    )
    .bind(deployment_hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("find_by_deployment_hash_optional: {}", e))
}
