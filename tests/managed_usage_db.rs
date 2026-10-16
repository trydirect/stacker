//! Persistence of the Managed server usage behind a Quick Deploy.
//! Scenarios: config/shared-fixtures/features/quick-deploy-managed-server-billing.feature

mod common;

use chrono::{Datelike, Duration, NaiveDate, Utc};
use stacker::db::managed_usage::{self, NewManagedUsage};

fn new_usage(hash: &str, user_id: &str) -> NewManagedUsage {
    NewManagedUsage {
        deployment_hash: hash.to_string(),
        user_id: user_id.to_string(),
        stack: "local-ai-packaged".to_string(),
        server_type: "cpx32".to_string(),
        daily_rate_minor: 86,
        monthly_cap_minor: 2600,
    }
}

fn today() -> NaiveDate {
    Utc::now().date_naive()
}

#[tokio::test]
async fn a_deploy_records_pending_usage_and_success_makes_it_active() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let row = managed_usage::insert_pending(&app.db_pool, &new_usage("dep-start", "u-1"))
        .await
        .expect("insert");
    assert_eq!(row.status, "pending");
    assert!(row.started_at.is_none());

    let started = managed_usage::start(&app.db_pool, "dep-start")
        .await
        .expect("start")
        .expect("pending row started");
    assert_eq!(started.status, "active");
    assert!(started.started_at.is_some());

    // Starting twice changes nothing.
    assert!(managed_usage::start(&app.db_pool, "dep-start")
        .await
        .expect("start again")
        .is_none());
}

#[tokio::test]
async fn a_failed_deploy_closes_the_usage_without_starting_it() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    managed_usage::insert_pending(&app.db_pool, &new_usage("dep-fail", "u-1"))
        .await
        .expect("insert");
    let closed = managed_usage::close(&app.db_pool, "dep-fail", "deploy_failed")
        .await
        .expect("close")
        .expect("pending row closed");
    assert_eq!(closed.status, "closed");
    assert_eq!(closed.outcome.as_deref(), Some("deploy_failed"));
    assert!(closed.started_at.is_none());
    assert!(managed_usage::list_due(&app.db_pool, today(), 100)
        .await
        .expect("due")
        .iter()
        .all(|r| r.deployment_hash != "dep-fail"));
}

#[tokio::test]
async fn running_servers_are_due_until_their_day_is_recorded() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    managed_usage::insert_pending(&app.db_pool, &new_usage("dep-due", "u-1"))
        .await
        .expect("insert");
    managed_usage::insert_pending(&app.db_pool, &new_usage("dep-pending", "u-1"))
        .await
        .expect("insert");
    let row = managed_usage::start(&app.db_pool, "dep-due")
        .await
        .expect("start")
        .unwrap();

    let due = managed_usage::list_due(&app.db_pool, today(), 100)
        .await
        .expect("due");
    assert!(due.iter().any(|r| r.deployment_hash == "dep-due"));
    assert!(
        due.iter().all(|r| r.deployment_hash != "dep-pending"),
        "a pending deploy is not billed"
    );

    let period_start = today().with_day0(0).unwrap();
    assert!(
        managed_usage::record_day(&app.db_pool, row.id, period_start, 86, today())
            .await
            .expect("record")
    );
    // The same day again is refused, so a retry cannot count it twice.
    assert!(
        !managed_usage::record_day(&app.db_pool, row.id, period_start, 172, today())
            .await
            .expect("record again")
    );

    let after = managed_usage::fetch_by_hash(&app.db_pool, "dep-due")
        .await
        .expect("fetch")
        .unwrap();
    assert_eq!(after.period_reported_minor, 86);
    assert_eq!(after.last_reported_on, Some(today()));
    assert!(managed_usage::list_due(&app.db_pool, today(), 100)
        .await
        .expect("due")
        .iter()
        .all(|r| r.deployment_hash != "dep-due"));
}

#[tokio::test]
async fn a_deleted_server_stays_due_only_up_to_its_deletion_day() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    managed_usage::insert_pending(&app.db_pool, &new_usage("dep-stop", "u-1"))
        .await
        .expect("insert");
    managed_usage::start(&app.db_pool, "dep-stop")
        .await
        .expect("start");
    let stopped = managed_usage::stop(&app.db_pool, "dep-stop")
        .await
        .expect("stop")
        .expect("active row stopped");
    assert_eq!(stopped.status, "stopped");
    assert!(stopped.stopped_at.is_some());

    // Not yet reported for the deletion day: still due, until that day.
    let due = managed_usage::list_due(&app.db_pool, today() + Duration::days(5), 100)
        .await
        .expect("due");
    let row = due
        .iter()
        .find(|r| r.deployment_hash == "dep-stop")
        .expect("due for its last day");
    assert_eq!(row.report_until(today() + Duration::days(5)), today());

    let period_start = today().with_day0(0).unwrap();
    managed_usage::record_day(&app.db_pool, row.id, period_start, 86, today())
        .await
        .expect("record");
    assert!(
        managed_usage::list_due(&app.db_pool, today() + Duration::days(5), 100)
            .await
            .expect("due")
            .iter()
            .all(|r| r.deployment_hash != "dep-stop")
    );
}

#[tokio::test]
async fn an_unpaid_invoice_suspends_every_active_server_of_the_buyer() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    for hash in ["dep-s1", "dep-s2"] {
        managed_usage::insert_pending(&app.db_pool, &new_usage(hash, "u-owes"))
            .await
            .expect("insert");
        managed_usage::start(&app.db_pool, hash)
            .await
            .expect("start");
    }
    managed_usage::insert_pending(&app.db_pool, &new_usage("dep-other", "u-pays"))
        .await
        .expect("insert");
    managed_usage::start(&app.db_pool, "dep-other")
        .await
        .expect("start");

    let mut suspended: Vec<String> = managed_usage::suspend_for_user(&app.db_pool, "u-owes")
        .await
        .expect("suspend")
        .into_iter()
        .map(|r| r.deployment_hash)
        .collect();
    suspended.sort();
    assert_eq!(suspended, vec!["dep-s1", "dep-s2"]);

    let other = managed_usage::fetch_by_hash(&app.db_pool, "dep-other")
        .await
        .expect("fetch")
        .unwrap();
    assert_eq!(other.status, "active");
}

#[tokio::test]
async fn servers_suspended_for_3_days_are_listed_for_deletion() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    managed_usage::insert_pending(&app.db_pool, &new_usage("dep-old", "u-late"))
        .await
        .expect("insert");
    managed_usage::start(&app.db_pool, "dep-old")
        .await
        .expect("start");
    managed_usage::suspend_for_user(&app.db_pool, "u-late")
        .await
        .expect("suspend");

    let now = Utc::now();
    assert!(
        managed_usage::list_suspended_before(&app.db_pool, now - Duration::days(3))
            .await
            .expect("list")
            .iter()
            .all(|r| r.deployment_hash != "dep-old")
    );
    assert!(
        managed_usage::list_suspended_before(&app.db_pool, now + Duration::seconds(1))
            .await
            .expect("list")
            .iter()
            .any(|r| r.deployment_hash == "dep-old")
    );

    let closed = managed_usage::close_deleted_unpaid(&app.db_pool, "dep-old")
        .await
        .expect("close")
        .expect("suspended row closed");
    assert_eq!(closed.status, "closed");
    assert_eq!(closed.outcome.as_deref(), Some("server_deleted_unpaid"));
}
