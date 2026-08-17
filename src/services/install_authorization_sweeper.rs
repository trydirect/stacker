//! Background sweeper for expired per-install billing authorizations.
//!
//! An authorization can end up "stuck" in the `authorized` state if the
//! deploy pipeline never confirms via `deploy_complete_handler` — the
//! agent crashed, the server never came up, the user Ctrl-C'd before
//! confirmation, etc. The sweeper voids these once they pass their TTL
//! so the buyer isn't stuck holding a live authorization on their card
//! and stacker's ledger reconciles with user_service's.
//!
//! Correctness invariant: this sweeper is a **cleanup tool**, not the
//! source of truth. user_service's own `expires_at` on the underlying
//! payment intent auto-voids independently — if the sweeper is down or
//! never runs, the authorization still lapses at the payment provider.
//! What the sweeper adds is prompt DB-state reconciliation.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sqlx::PgPool;

use crate::connectors::errors::ConnectorError;
use crate::connectors::user_service::UserServiceConnector;
use crate::db;

/// How often the sweeper checks for expired authorizations.
const TICK: Duration = Duration::from_secs(60);

/// How far past `expires_at` we let a row linger before voiding, giving
/// deploy-complete a grace window to arrive after the TTL fires.
const GRACE_SECS: i64 = 300;

/// How many rows we void per tick. Voids are one-round-trip each and
/// user_service will rate-limit us if we're too aggressive.
const BATCH_LIMIT: i64 = 500;

/// Grace period for failed daily charges before suspending (24 hours).
const DAILY_CHARGE_GRACE_SECS: i64 = 86400;

/// Time after suspension before deleting the server (3 days).
const SUSPENSION_DELETE_SECS: i64 = 259200;

pub fn spawn(
    pg_pool: PgPool,
    user_service: Arc<dyn UserServiceConnector>,
    per_install_enabled: bool,
) {
    if !per_install_enabled {
        tracing::info!("install_authorization_sweeper skipped: per_install billing disabled");
        return;
    }
    tokio::spawn(async move {
        tracing::info!("install_authorization_sweeper started (tick={:?})", TICK);
        loop {
            tokio::time::sleep(TICK).await;
            if let Err(err) = tick_once(&pg_pool, user_service.as_ref()).await {
                tracing::warn!("install_authorization_sweeper tick error: {}", err);
            }
        }
    });
}

async fn tick_once(pool: &PgPool, user_service: &dyn UserServiceConnector) -> Result<(), String> {
    // 1. Void expired per_install authorizations
    let cutoff = Utc::now() - chrono::Duration::seconds(GRACE_SECS);
    let expired =
        db::marketplace_billing::list_expired_authorized(pool, cutoff, BATCH_LIMIT).await?;
    if !expired.is_empty() {
        tracing::info!(
            "install_authorization_sweeper: voiding {} expired authorization(s)",
            expired.len()
        );
        let service_token = std::env::var("STACKER_SERVICE_TOKEN").unwrap_or_default();
        for row in expired {
            match user_service
                .void_install_charge(&service_token, &row.authorization_id, "expired")
                .await
            {
                Ok(_) => {
                    if let Err(err) =
                        db::marketplace_billing::mark_voided(pool, &row.authorization_id, "expired")
                            .await
                    {
                        tracing::warn!(
                            "sweeper mark_voided DB error for {}: {}",
                            row.authorization_id,
                            err
                        );
                    }
                }
                Err(ConnectorError::Conflict(_)) => {
                    tracing::info!(
                        "sweeper reconciling {} as captured (user_service returned 409)",
                        row.authorization_id
                    );
                    if let Err(err) =
                        db::marketplace_billing::mark_captured(pool, &row.authorization_id).await
                    {
                        tracing::warn!(
                            "sweeper mark_captured DB error for {}: {}",
                            row.authorization_id,
                            err
                        );
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        "sweeper void failed for {}: {} (will retry next tick)",
                        row.authorization_id,
                        err
                    );
                }
            }
        }
    }

    // 2. Daily billing sweep for deployment_daily authorizations
    let daily_candidates =
        db::marketplace_billing::list_daily_sweep_candidates(pool, BATCH_LIMIT).await?;
    if !daily_candidates.is_empty() {
        tracing::info!(
            "deployment_daily sweeper: charging {} authorization(s)",
            daily_candidates.len()
        );
        let service_token = std::env::var("STACKER_SERVICE_TOKEN").unwrap_or_default();
        for row in daily_candidates {
            let daily_rate = row.daily_rate.unwrap_or(0.0);
            let monthly_cap = row.monthly_cap.unwrap_or(0.0);
            let total_charged = row.total_charged_minor.unwrap_or(0) as f64 / 100.0;

            // Check if monthly cap reached
            if total_charged >= monthly_cap && monthly_cap > 0.0 {
                tracing::debug!(
                    "deployment_daily: {} reached monthly cap ({}/{}), skipping",
                    row.authorization_id,
                    total_charged,
                    monthly_cap
                );
                continue;
            }

            // Check if server was deleted
            if row.server_deleted_at.is_some() {
                // Void remaining hold
                if let Err(err) = user_service
                    .void_install_charge(&service_token, &row.authorization_id, "server_deleted")
                    .await
                {
                    tracing::warn!(
                        "deployment_daily void after delete failed for {}: {}",
                        row.authorization_id,
                        err
                    );
                }
                continue;
            }

            // Check if suspended and past deletion threshold
            if let Some(suspended_at) = row.suspended_at {
                let suspension_age = Utc::now().signed_duration_since(suspended_at);
                if suspension_age.num_seconds() > SUSPENSION_DELETE_SECS {
                    tracing::info!(
                        "deployment_daily: deleting suspended server for {} (suspended {}s ago)",
                        row.authorization_id,
                        suspension_age.num_seconds()
                    );
                    // Void the authorization — server will be cleaned up separately
                    if let Err(err) = user_service
                        .void_install_charge(
                            &service_token,
                            &row.authorization_id,
                            "suspension_expired",
                        )
                        .await
                    {
                        tracing::warn!(
                            "deployment_daily void after suspension expired failed for {}: {}",
                            row.authorization_id,
                            err
                        );
                    }
                    if let Err(err) = db::marketplace_billing::mark_voided(
                        pool,
                        &row.authorization_id,
                        "suspension_expired",
                    )
                    .await
                    {
                        tracing::warn!("mark_voided error: {}", err);
                    }
                }
                continue;
            }

            // Attempt daily charge
            let charged_minor = (daily_rate * 100.0).round() as i64;
            let deployment_hash = row.deployment_hash.clone().unwrap_or_default();

            match user_service
                .daily_capture_install_charge(
                    &service_token,
                    &row.authorization_id,
                    charged_minor,
                    &deployment_hash,
                )
                .await
            {
                Ok(_) => {
                    if let Err(err) = db::marketplace_billing::mark_daily_charged(
                        pool,
                        &row.authorization_id,
                        charged_minor,
                    )
                    .await
                    {
                        tracing::warn!(
                            "deployment_daily mark_daily_charged error for {}: {}",
                            row.authorization_id,
                            err
                        );
                    }
                    tracing::info!(
                        "deployment_daily: charged ${:.2} for {} (total: ${:.2}/${:.2})",
                        daily_rate,
                        row.authorization_id,
                        (row.total_charged_minor.unwrap_or(0) + charged_minor) as f64 / 100.0,
                        monthly_cap,
                    );
                }
                Err(ConnectorError::Conflict(_)) => {
                    // Already captured or voided — reconcile
                    tracing::info!(
                        "deployment_daily: reconciling {} (user_service returned 409)",
                        row.authorization_id
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        "deployment_daily: daily charge failed for {}: {}",
                        row.authorization_id,
                        err
                    );
                    // Check if grace period expired (last charge was 24+ hours ago)
                    if let Some(last_charge) = row.last_daily_charge_at {
                        let grace_age = Utc::now().signed_duration_since(last_charge);
                        if grace_age.num_seconds() > DAILY_CHARGE_GRACE_SECS {
                            tracing::info!(
                                "deployment_daily: suspending {} (grace period expired)",
                                row.authorization_id
                            );
                            if let Err(err) =
                                db::marketplace_billing::mark_suspended(pool, &row.authorization_id)
                                    .await
                            {
                                tracing::warn!("mark_suspended error: {}", err);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
