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

pub fn spawn(
    pg_pool: PgPool,
    user_service: Arc<dyn UserServiceConnector>,
    per_install_enabled: bool,
) {
    if !per_install_enabled {
        tracing::info!(
            "install_authorization_sweeper skipped: per_install billing disabled"
        );
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

async fn tick_once(
    pool: &PgPool,
    user_service: &dyn UserServiceConnector,
) -> Result<(), String> {
    let cutoff = Utc::now() - chrono::Duration::seconds(GRACE_SECS);
    let expired =
        db::marketplace_billing::list_expired_authorized(pool, cutoff, BATCH_LIMIT).await?;
    if expired.is_empty() {
        return Ok(());
    }
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
                // user_service says the authorization is not in the
                // `authorized` state — most likely already captured out
                // of band. Reconcile our local view.
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
    Ok(())
}
