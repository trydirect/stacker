//! Runtime validation of the backup db layer (WS-1 1.2). The sqlx calls use
//! the function form, so this exercises them against the real schema.

mod common;

use stacker::db;

#[tokio::test]
async fn backup_destination_job_and_run_lifecycle() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    let pool = &app.db_pool;
    let project_id = common::create_test_project(pool, common::USER_A_ID).await;

    // Destination
    let dest = db::backup::insert_destination(
        pool,
        common::USER_A_ID,
        "primary",
        "https://backup.example",
        "backup/primary",
    )
    .await
    .expect("insert destination");
    assert_eq!(dest.name, "primary");
    assert!(db::backup::fetch_destination(pool, dest.id).await.unwrap().is_some());
    assert_eq!(db::backup::list_destinations(pool, common::USER_A_ID).await.unwrap().len(), 1);

    // Job
    let job = db::backup::insert_job(
        pool,
        common::USER_A_ID,
        project_id,
        dest.id,
        "nightly",
        "postgres",
        "shop-postgres-1",
        Some("shop"),
        "0 3 * * *",
        7,
        4,
    )
    .await
    .expect("insert job");
    assert_eq!(job.retention_daily, 7);
    assert_eq!(db::backup::list_jobs_by_project(pool, project_id).await.unwrap().len(), 1);
    assert_eq!(db::backup::list_enabled_jobs(pool).await.unwrap().len(), 1);

    // Run lifecycle: pending -> running -> uploaded
    let run = db::backup::insert_run(pool, job.id, project_id).await.expect("insert run");
    assert_eq!(run.status, "pending");

    db::backup::mark_run_running(pool, run.id).await.expect("running");
    let r = db::backup::fetch_run(pool, run.id).await.unwrap().unwrap();
    assert_eq!(r.status, "running");
    assert!(r.started_at.is_some());

    db::backup::mark_run_uploaded(pool, run.id, 1024, "abc123", "f-1").await.expect("uploaded");
    let r = db::backup::fetch_run(pool, run.id).await.unwrap().unwrap();
    assert_eq!(r.status, "uploaded");
    assert_eq!(r.size_bytes, Some(1024));
    assert_eq!(r.remote_file_id.as_deref(), Some("f-1"));

    // A second run that fails does not show up as finished.
    let run2 = db::backup::insert_run(pool, job.id, project_id).await.expect("insert run2");
    db::backup::mark_run_failed(pool, run2.id, "boom").await.expect("failed");
    let r2 = db::backup::fetch_run(pool, run2.id).await.unwrap().unwrap();
    assert_eq!(r2.status, "failed");
    assert_eq!(r2.error.as_deref(), Some("boom"));

    let finished = db::backup::list_finished_runs(pool, job.id).await.unwrap();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].id, run.id);

    assert_eq!(db::backup::list_runs_by_job(pool, job.id).await.unwrap().len(), 2);

    // Cleanup
    db::backup::mark_runs_pruned(pool, &[run.id]).await.expect("prune");
    let r = db::backup::fetch_run(pool, run.id).await.unwrap().unwrap();
    assert_eq!(r.status, "pruned");

    db::backup::delete_job(pool, job.id).await.expect("delete job");
    assert!(db::backup::fetch_job(pool, job.id).await.unwrap().is_none());
    db::backup::delete_destination(pool, dest.id).await.expect("delete dest");
    assert!(db::backup::fetch_destination(pool, dest.id).await.unwrap().is_none());
}
