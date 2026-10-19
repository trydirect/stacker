//! The daily report of the Managed server usage behind a Quick Deploy.
//! Scenarios: config/shared-fixtures/features/quick-deploy-managed-server-billing.feature

mod common;

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use sqlx::PgPool;
use stacker::connectors::user_service::{ManagedBillingEvent, ManagedUsageReport};
use stacker::db::managed_usage::{self, NewManagedUsage};
use stacker::services::managed_usage_sweeper::{tick_once, UsageSink};

#[derive(Default)]
struct Recorder {
    reports: Mutex<Vec<ManagedUsageReport>>,
    events: Mutex<Vec<ManagedBillingEvent>>,
    fail_reports: bool,
}

#[async_trait]
impl UsageSink for Recorder {
    async fn report(&self, usage: &ManagedUsageReport) -> Result<(), String> {
        if self.fail_reports {
            return Err("payment unreachable".to_string());
        }
        self.reports.lock().unwrap().push(usage.clone());
        Ok(())
    }

    async fn event(&self, event: &ManagedBillingEvent) -> Result<(), String> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

impl Recorder {
    fn reports_for(&self, hash: &str) -> Vec<(String, i64)> {
        self.reports
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.deployment_hash == hash)
            .map(|r| (r.identifier.clone(), r.value_minor))
            .collect()
    }
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn noon(d: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&d.and_hms_opt(12, 0, 0).unwrap())
}

/// A running deployment whose usage started at noon on `started`.
async fn running(pool: &PgPool, hash: &str, rate: i64, cap: i64, started: NaiveDate) {
    managed_usage::insert_pending(
        pool,
        &NewManagedUsage {
            deployment_hash: hash.to_string(),
            user_id: "u-1".to_string(),
            stack: "local-ai-packaged".to_string(),
            server_type: "cpx32".to_string(),
            daily_rate_minor: rate,
            monthly_cap_minor: cap,
        },
    )
    .await
    .unwrap();
    managed_usage::start(pool, hash).await.unwrap();
    sqlx::query("UPDATE managed_usage SET started_at = $2 WHERE deployment_hash = $1")
        .bind(hash)
        .bind(noon(started))
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn each_running_day_is_sent_once() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let today = day(2026, 10, 16);
    running(&app.db_pool, "sw-once", 86, 2600, today).await;
    let sink = Recorder::default();

    tick_once(&app.db_pool, &sink, today, noon(today))
        .await
        .unwrap();
    tick_once(&app.db_pool, &sink, today, noon(today))
        .await
        .unwrap();

    assert_eq!(
        sink.reports_for("sw-once"),
        vec![("sw-once:2026-10-16".to_string(), 86)]
    );
}

#[tokio::test]
async fn missed_days_are_caught_up_in_order() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let today = day(2026, 10, 16);
    running(&app.db_pool, "sw-catch", 86, 2600, day(2026, 10, 13)).await;
    let sink = Recorder::default();

    tick_once(&app.db_pool, &sink, today, noon(today))
        .await
        .unwrap();

    assert_eq!(
        sink.reports_for("sw-catch"),
        vec![
            ("sw-catch:2026-10-13".to_string(), 86),
            ("sw-catch:2026-10-14".to_string(), 86),
            ("sw-catch:2026-10-15".to_string(), 86),
            ("sw-catch:2026-10-16".to_string(), 86),
        ]
    );
    // Past days are stamped within their own day; today is stamped now.
    let reports = sink.reports.lock().unwrap();
    let stamp = |id: &str| {
        reports
            .iter()
            .find(|r| r.identifier == id)
            .and_then(|r| r.timestamp)
            .unwrap()
    };
    let d13 = stamp("sw-catch:2026-10-13");
    assert!(d13 > noon(day(2026, 10, 13)).timestamp() && d13 < noon(day(2026, 10, 14)).timestamp());
    assert_eq!(stamp("sw-catch:2026-10-16"), noon(today).timestamp());
}

#[tokio::test]
async fn the_month_never_goes_over_the_cap() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let today = day(2026, 10, 3);
    running(&app.db_pool, "sw-cap", 86, 200, day(2026, 10, 1)).await;
    let sink = Recorder::default();

    tick_once(&app.db_pool, &sink, today, noon(today))
        .await
        .unwrap();

    assert_eq!(
        sink.reports_for("sw-cap")
            .iter()
            .map(|(_, v)| *v)
            .collect::<Vec<_>>(),
        vec![86, 86, 28]
    );
}

#[tokio::test]
async fn a_failed_send_is_retried_on_the_next_tick() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let today = day(2026, 10, 16);
    running(&app.db_pool, "sw-retry", 86, 2600, today).await;

    let failing = Recorder {
        fail_reports: true,
        ..Recorder::default()
    };
    tick_once(&app.db_pool, &failing, today, noon(today))
        .await
        .unwrap();
    let row = managed_usage::fetch_by_hash(&app.db_pool, "sw-retry")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.last_reported_on, None, "an unsent day is not marked");

    let sink = Recorder::default();
    tick_once(&app.db_pool, &sink, today, noon(today))
        .await
        .unwrap();
    assert_eq!(
        sink.reports_for("sw-retry"),
        vec![("sw-retry:2026-10-16".to_string(), 86)]
    );
}

#[tokio::test]
async fn a_server_suspended_for_3_days_is_handed_to_the_admins() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let today = day(2026, 10, 16);
    for (hash, suspended_days_ago) in [("sw-late", 4), ("sw-recent", 1)] {
        running(&app.db_pool, hash, 86, 2600, day(2026, 10, 1)).await;
        managed_usage::set_server_id(&app.db_pool, hash, 900 + suspended_days_ago)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE managed_usage SET status = 'suspended', suspended_at = $2 WHERE deployment_hash = $1",
        )
        .bind(hash)
        .bind(noon(today) - Duration::days(suspended_days_ago))
        .execute(&app.db_pool)
        .await
        .unwrap();
    }
    let sink = Recorder::default();

    tick_once(&app.db_pool, &sink, today, noon(today))
        .await
        .unwrap();

    let late = managed_usage::fetch_by_hash(&app.db_pool, "sw-late")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(late.status, "closed");
    assert_eq!(late.outcome.as_deref(), Some("server_delete_due"));
    let recent = managed_usage::fetch_by_hash(&app.db_pool, "sw-recent")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recent.status, "suspended");

    let events = sink.events.lock().unwrap();
    let due: Vec<_> = events
        .iter()
        .filter(|e| e.event == "server_delete_due")
        .collect();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].deployment_hash, "sw-late");
    assert_eq!(due[0].metadata.as_ref().unwrap()["server_id"], 904);

    // Days are reported only up to the suspension day.
    let last = sink
        .reports_for("sw-late")
        .last()
        .map(|(id, _)| id.clone())
        .unwrap();
    assert_eq!(last, "sw-late:2026-10-12");
}
