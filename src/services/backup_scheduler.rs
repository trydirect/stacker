//! Backup scheduler (WS-1 1.4): periodically enqueue due backup runs and
//! apply the retention policy.
//!
//! Due-detection is data-driven: a job fires when the next cron time after its
//! last run has arrived. The enqueue reuses the WS-1 1.2 path (typed plan →
//! `backup` agent command). Retention (B2: 7 daily + 4 weekly) marks pruned
//! runs; deleting the archives from storage is the connector's job (WS-1 1.3).

use crate::db;
use crate::models::BackupJob;
use crate::services::backup as backup_service;
use backup_core::runs_to_prune;
use chrono::{DateTime, Utc};
use cron::Schedule;
use sqlx::PgPool;
use std::str::FromStr;
use std::time::Duration;

/// How often the scheduler looks for due jobs.
const TICK: Duration = Duration::from_secs(60);

pub fn spawn(pg_pool: PgPool) {
    tokio::spawn(async move {
        tracing::info!("backup_scheduler started (tick={:?})", TICK);
        loop {
            // Sleep first: startup is busy, and nothing here is urgent.
            tokio::time::sleep(TICK).await;
            if let Err(err) = tick(&pg_pool).await {
                tracing::warn!("backup_scheduler tick error: {}", err);
            }
        }
    });
}

/// Is the job's schedule due at `now`, given it last produced a run at
/// `last_run`? True when the next cron time after `last_run` has arrived.
fn is_due(schedule: &str, last_run: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    match parse_schedule(schedule) {
        Ok(sched) => sched
            .after(&last_run)
            .next()
            .map(|next| next <= now)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Parse a cron schedule. The `cron` crate expects a seconds-first expression
/// (`sec min hour dom month dow [year]`); standard 5-field schedules
/// (`min hour dom month dow`) are accepted by defaulting seconds to 0.
fn parse_schedule(schedule: &str) -> Result<Schedule, cron::error::Error> {
    let fields = schedule.split_whitespace().count();
    let normalized = if fields == 5 {
        format!("0 {}", schedule)
    } else {
        schedule.to_string()
    };
    Schedule::from_str(&normalized)
}

async fn tick(pool: &PgPool) -> Result<(), String> {
    let jobs = db::backup::list_enabled_jobs(pool).await?;
    let now = Utc::now();
    let token_url = std::env::var("BACKUP_TOKEN_URL").unwrap_or_default();

    for job in jobs {
        // Retention runs every tick regardless of whether the job fires.
        apply_retention(pool, &job).await;

        // Due? The next cron time after the last run must have arrived.
        let last_run = db::backup::last_run_at(pool, job.id)
            .await?
            .unwrap_or(job.created_at);
        if !is_due(&job.schedule, last_run, now) {
            continue;
        }
        if token_url.is_empty() {
            tracing::warn!(
                job_id = job.id,
                "BACKUP_TOKEN_URL not set; skipping due backup"
            );
            continue;
        }
        // Address the agent via the project's deployment.
        let deployments = db::deployment::fetch_by_project_id(pool, job.project_id)
            .await
            .unwrap_or_default();
        let Some(deployment) = deployments.into_iter().next() else {
            tracing::warn!(
                job_id = job.id,
                "project has no deployment; skipping backup"
            );
            continue;
        };
        let Some(dest) = db::backup::fetch_destination(pool, job.destination_id)
            .await
            .unwrap_or(None)
        else {
            tracing::warn!(job_id = job.id, "destination missing; skipping backup");
            continue;
        };

        match backup_service::enqueue_backup_run(pool, deployment.id, &job, &dest, &token_url).await
        {
            Ok((run_id, command_id)) => tracing::info!(
                job_id = job.id,
                run_id,
                command_id,
                "scheduled backup run queued"
            ),
            Err(err) => tracing::warn!(job_id = job.id, "failed to queue backup: {}", err),
        }
    }
    Ok(())
}

/// Apply the retention policy to a job's finished runs (B2). Pruned archives
/// are deleted from storage by the connector (WS-1 1.3); here runs are marked
/// pruned.
async fn apply_retention(pool: &PgPool, job: &BackupJob) {
    let runs = match db::backup::list_finished_runs(pool, job.id).await {
        Ok(runs) => runs,
        Err(err) => {
            tracing::warn!(job_id = job.id, "retention listing failed: {}", err);
            return;
        }
    };
    let to_prune = runs_to_prune(
        &runs,
        job.retention_daily as usize,
        job.retention_weekly as usize,
    );
    if to_prune.is_empty() {
        return;
    }

    // Delete the pruned archives from storage (best-effort; the run is marked
    // pruned regardless so retention is idempotent).
    let token_url = std::env::var("BACKUP_TOKEN_URL").unwrap_or_default();
    let client_id = std::env::var("BACKUP_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("BACKUP_CLIENT_SECRET").unwrap_or_default();
    if !token_url.is_empty() && !client_id.is_empty() {
        if let Ok(Some(dest)) = db::backup::fetch_destination(pool, job.destination_id).await {
            let mut client = crate::services::storage::StorageClient::new(
                &dest.endpoint,
                &token_url,
                &client_id,
                &client_secret,
            );
            for run in &runs {
                if !to_prune.contains(&run.id) {
                    continue;
                }
                if let Some(file_id) = &run.remote_file_id {
                    match client.delete_file(file_id).await {
                        Ok(()) => {
                            tracing::debug!(job_id = job.id, file_id, "pruned archive deleted")
                        }
                        Err(err) => tracing::warn!(
                            job_id = job.id,
                            file_id,
                            "failed to delete pruned archive: {}",
                            err
                        ),
                    }
                }
            }
        }
    }

    match db::backup::mark_runs_pruned(pool, &to_prune).await {
        Ok(()) => tracing::info!(
            job_id = job.id,
            pruned = to_prune.len(),
            "retention applied"
        ),
        Err(err) => tracing::warn!(job_id = job.id, "retention prune failed: {}", err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_is_due_daily_schedule() {
        let last = Utc.with_ymd_and_hms(2026, 11, 25, 3, 0, 0).unwrap();
        // Not due a minute before the next 03:00.
        let before = Utc.with_ymd_and_hms(2026, 11, 26, 2, 59, 0).unwrap();
        assert!(!is_due("0 3 * * *", last, before));
        // Due at/after the next 03:00.
        let after = Utc.with_ymd_and_hms(2026, 11, 26, 3, 0, 0).unwrap();
        assert!(is_due("0 3 * * *", last, after));
    }

    #[test]
    fn test_is_due_invalid_schedule_is_never_due() {
        let now = Utc.with_ymd_and_hms(2026, 11, 26, 3, 0, 0).unwrap();
        assert!(!is_due("not a cron", now, now));
    }
}
