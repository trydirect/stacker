//! Daily report of the Managed server usage behind a Quick Deploy.
//!
//! Each tick sends every day not yet reported, up to today for a running
//! server and up to the stop day for a deleted or suspended one, as one
//! meter event per day through the User Service. A server suspended for 3
//! days is handed to the admins (`server_delete_due`): nothing deletes a VM
//! automatically. Contract: shared-fixtures/api-contracts/managed-server-billing.json.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use sqlx::PgPool;

use crate::connectors::user_service::{
    ManagedBillingEvent, ManagedUsageReport, UserServiceConnector,
};
use crate::db::managed_usage;
use crate::services::managed_usage::{identifier, next_report, UsageState};

const TICK: StdDuration = StdDuration::from_secs(600);
const BATCH_LIMIT: i64 = 500;
/// Stripe accepts a meter event up to 35 days old; older days are skipped.
const MAX_DAY_AGE: i64 = 34;
/// A suspended server is handed to the admins after this long.
pub const SUSPENSION_HANDOFF_DAYS: i64 = 3;

/// Where usage and billing events go. The User Service in production.
#[async_trait]
pub trait UsageSink: Send + Sync {
    async fn report(&self, usage: &ManagedUsageReport) -> Result<(), String>;
    async fn event(&self, event: &ManagedBillingEvent) -> Result<(), String>;
}

pub struct UserServiceSink {
    pub user_service: Arc<dyn UserServiceConnector>,
    pub service_token: String,
}

#[async_trait]
impl UsageSink for UserServiceSink {
    async fn report(&self, usage: &ManagedUsageReport) -> Result<(), String> {
        self.user_service
            .report_managed_usage(&self.service_token, usage)
            .await
            .map_err(|e| e.to_string())
    }

    async fn event(&self, event: &ManagedBillingEvent) -> Result<(), String> {
        self.user_service
            .record_managed_billing_event(&self.service_token, event)
            .await
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TickSummary {
    pub days_reported: usize,
    pub handed_to_admins: usize,
}

pub fn spawn(pg_pool: PgPool, user_service: Arc<dyn UserServiceConnector>) {
    let sink = UserServiceSink {
        user_service,
        service_token: std::env::var("STACKER_SERVICE_TOKEN").unwrap_or_default(),
    };
    tokio::spawn(async move {
        tracing::info!("managed_usage_sweeper started (tick={:?})", TICK);
        loop {
            let now = Utc::now();
            if let Err(err) = tick_once(&pg_pool, &sink, now.date_naive(), now).await {
                tracing::warn!("managed_usage_sweeper tick error: {}", err);
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

pub async fn tick_once(
    pool: &PgPool,
    sink: &dyn UsageSink,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Result<TickSummary, String> {
    let mut summary = TickSummary::default();

    for row in managed_usage::list_due(pool, today, BATCH_LIMIT).await? {
        let Some(started_at) = row.started_at else {
            continue;
        };
        let mut state = UsageState {
            rate_minor: row.daily_rate_minor,
            cap_minor: row.monthly_cap_minor,
            started_on: started_at.date_naive(),
            period_start: row.period_start,
            period_reported_minor: row.period_reported_minor,
            last_reported_on: row.last_reported_on,
        };
        let until = row.report_until(today);
        let oldest = today - Duration::days(MAX_DAY_AGE);
        while let Some(report) = next_report(&state, until) {
            if report.day < oldest {
                tracing::error!(
                    deployment_hash = %row.deployment_hash,
                    day = %report.day,
                    "managed usage day too old for the Stripe meter; skipped"
                );
            } else if report.value_minor > 0 {
                let usage = ManagedUsageReport {
                    user_id: row.user_id.clone(),
                    deployment_hash: row.deployment_hash.clone(),
                    value_minor: report.value_minor,
                    identifier: identifier(&row.deployment_hash, report.day),
                    timestamp: Some(timestamp_for(report.day, today, now, started_at)),
                };
                if let Err(err) = sink.report(&usage).await {
                    // Not marked: the next tick sends this day again.
                    tracing::warn!(
                        deployment_hash = %row.deployment_hash,
                        day = %report.day,
                        error = %err,
                        "managed usage report failed"
                    );
                    break;
                }
                summary.days_reported += 1;
            }
            if !managed_usage::record_day(
                pool,
                row.id,
                report.period_start,
                report.period_reported_minor,
                report.day,
            )
            .await?
            {
                break;
            }
            state.period_start = Some(report.period_start);
            state.period_reported_minor = report.period_reported_minor;
            state.last_reported_on = Some(report.day);
        }
    }

    let cutoff = now - Duration::days(SUSPENSION_HANDOFF_DAYS);
    for row in managed_usage::list_suspended_before(pool, cutoff).await? {
        // The alert first: if it does not get through, the row stays
        // suspended and the next tick tries again.
        let event = ManagedBillingEvent {
            user_id: row.user_id.clone(),
            deployment_hash: row.deployment_hash.clone(),
            event: "server_delete_due".to_string(),
            amount_minor: None,
            currency: None,
            metadata: Some(serde_json::json!({
                "server_id": row.server_id,
                "stack": row.stack,
                "server_type": row.server_type,
                "suspended_at": row.suspended_at,
            })),
        };
        if let Err(err) = sink.event(&event).await {
            tracing::warn!(
                deployment_hash = %row.deployment_hash,
                error = %err,
                "server_delete_due event failed"
            );
            continue;
        }
        if managed_usage::mark_delete_due(pool, &row.deployment_hash)
            .await?
            .is_some()
        {
            summary.handed_to_admins += 1;
        }
    }

    Ok(summary)
}

/// Today's day is stamped now; a past day late in that day (but after the
/// usage started), so Stripe puts it in the right invoice period.
fn timestamp_for(
    day: NaiveDate,
    today: NaiveDate,
    now: DateTime<Utc>,
    started_at: DateTime<Utc>,
) -> i64 {
    if day >= today {
        return now.timestamp();
    }
    let late = day
        .and_hms_opt(23, 0, 0)
        .expect("23:00 exists")
        .and_utc()
        .max(started_at + Duration::seconds(1));
    late.min(now).timestamp()
}
