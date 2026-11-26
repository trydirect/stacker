//! Backup REST API (WS-1 1.5): destinations, jobs and runs. Owner-gated.

use crate::db;
use crate::forms::{
    mask_endpoint, CreateDestinationRequest, CreateJobRequest, DestinationResponse, JobResponse,
    RunResponse, UpdateJobRequest,
};
use crate::helpers::JsonResponse;
use crate::models;
use crate::routes::project::access::{self, Need};
use actix_web::{delete, get, patch, post, web, Responder, Result};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

/// Validate a job's source + schedule before touching the database.
fn validate_job(source: &str, target: &str, schedule: &str) -> Result<(), actix_web::Error> {
    if backup_core::DumpSource::parse(source).is_err() {
        return Err(JsonResponse::bad_request(
            "source must be one of: postgres, mysql, mongo, volume",
        ));
    }
    if target.trim().is_empty() {
        return Err(JsonResponse::bad_request("target must not be empty"));
    }
    if schedule.trim().is_empty() {
        return Err(JsonResponse::bad_request("schedule must not be empty"));
    }
    Ok(())
}

fn destination_response(dest: &models::BackupDestination) -> DestinationResponse {
    DestinationResponse {
        id: dest.id,
        name: dest.name.clone(),
        endpoint: mask_endpoint(&dest.endpoint),
        created_at: dest.created_at,
        updated_at: dest.updated_at,
    }
}

fn job_response(job: &models::BackupJob) -> JobResponse {
    JobResponse {
        id: job.id,
        project_id: job.project_id,
        destination_id: job.destination_id,
        name: job.name.clone(),
        source: job.source.clone(),
        target: job.target.clone(),
        database: job.database.clone(),
        schedule: job.schedule.clone(),
        retention_daily: job.retention_daily,
        retention_weekly: job.retention_weekly,
        enabled: job.enabled,
        created_at: job.created_at,
        updated_at: job.updated_at,
    }
}

fn run_response(run: &models::BackupRun) -> RunResponse {
    RunResponse {
        id: run.id,
        job_id: run.job_id,
        project_id: run.project_id,
        status: run.status.clone(),
        size_bytes: run.size_bytes,
        checksum: run.checksum.clone(),
        remote_file_id: run.remote_file_id.clone(),
        error: run.error.clone(),
        started_at: run.started_at,
        finished_at: run.finished_at,
        created_at: run.created_at,
    }
}

/// Load a job and verify the caller owns its project.
async fn fetch_owned_job(
    pool: &PgPool,
    user: &models::User,
    id: i32,
) -> Result<models::BackupJob, actix_web::Error> {
    let job = db::backup::fetch_job(pool, id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Backup job not found"))?;
    let project = db::project::fetch(pool, job.project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Backup job not found"))?;
    access::check(
        pool,
        &project,
        &user.id,
        Need::Owner,
        "Backup job not found",
    )
    .await?;
    Ok(job)
}

// ── Destinations ─────────────────────────────────────────────────────────

#[post("/destinations")]
pub async fn create_destination(
    user: web::ReqData<Arc<models::User>>,
    body: web::Json<CreateDestinationRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    if body.name.trim().is_empty() {
        return Err(JsonResponse::bad_request("name must not be empty"));
    }
    if body.endpoint.trim().is_empty() {
        return Err(JsonResponse::bad_request("endpoint must not be empty"));
    }
    let dest = db::backup::insert_destination(
        pg_pool.get_ref(),
        &user.id,
        &body.name,
        &body.endpoint,
        &body.vault_path,
    )
    .await
    .map_err(JsonResponse::internal_server_error)?;
    Ok(JsonResponse::build()
        .set_item(destination_response(&dest))
        .created("Backup destination created"))
}

#[get("/destinations")]
pub async fn list_destinations(
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let list = db::backup::list_destinations(pg_pool.get_ref(), &user.id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    let responses: Vec<DestinationResponse> = list.iter().map(destination_response).collect();
    Ok(JsonResponse::build()
        .set_list(responses)
        .ok("Backup destinations"))
}

#[delete("/destinations/{id}")]
pub async fn delete_destination(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();
    let dest = db::backup::fetch_destination(pg_pool.get_ref(), id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Backup destination not found"))?;
    if dest.user_id != user.id {
        return Err(JsonResponse::not_found("Backup destination not found"));
    }
    db::backup::delete_destination(pg_pool.get_ref(), id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    Ok(JsonResponse::build()
        .set_item(json!({ "deleted": true }))
        .ok("Backup destination deleted"))
}

// ── Jobs ────────────────────────────────────────────────────────────────

#[post("/jobs")]
pub async fn create_job(
    user: web::ReqData<Arc<models::User>>,
    body: web::Json<CreateJobRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    validate_job(&body.source, &body.target, &body.schedule)?;
    // The caller must own the project the job targets.
    let project = db::project::fetch(pg_pool.get_ref(), body.project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;
    access::check(
        pg_pool.get_ref(),
        &project,
        &user.id,
        Need::Owner,
        "Project not found",
    )
    .await?;

    let job = db::backup::insert_job(
        pg_pool.get_ref(),
        &user.id,
        body.project_id,
        body.destination_id,
        &body.name,
        &body.source,
        &body.target,
        body.database.as_deref(),
        &body.schedule,
        body.retention_daily,
        body.retention_weekly,
    )
    .await
    .map_err(JsonResponse::internal_server_error)?;
    Ok(JsonResponse::build()
        .set_item(job_response(&job))
        .created("Backup job created"))
}

#[derive(Deserialize)]
pub struct ListJobsQuery {
    #[serde(rename = "projectId")]
    project_id: Option<i32>,
}

#[get("/jobs")]
pub async fn list_jobs(
    user: web::ReqData<Arc<models::User>>,
    query: web::Query<ListJobsQuery>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let jobs = match query.project_id {
        Some(project_id) => {
            let project = db::project::fetch(pg_pool.get_ref(), project_id)
                .await
                .map_err(JsonResponse::internal_server_error)?
                .ok_or_else(|| JsonResponse::not_found("Project not found"))?;
            access::check(
                pg_pool.get_ref(),
                &project,
                &user.id,
                Need::Owner,
                "Project not found",
            )
            .await?;
            db::backup::list_jobs_by_project(pg_pool.get_ref(), project_id)
                .await
                .map_err(JsonResponse::internal_server_error)?
        }
        None => {
            let all = db::backup::list_enabled_jobs(pg_pool.get_ref())
                .await
                .map_err(JsonResponse::internal_server_error)?;
            all.into_iter().filter(|j| j.user_id == user.id).collect()
        }
    };
    let responses: Vec<JobResponse> = jobs.iter().map(job_response).collect();
    Ok(JsonResponse::build().set_list(responses).ok("Backup jobs"))
}

#[get("/jobs/{id}")]
pub async fn get_job(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let job = fetch_owned_job(pg_pool.get_ref(), &user, path.into_inner()).await?;
    Ok(JsonResponse::build()
        .set_item(job_response(&job))
        .ok("Backup job"))
}

#[patch("/jobs/{id}")]
pub async fn update_job(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    body: web::Json<UpdateJobRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();
    let existing = fetch_owned_job(pg_pool.get_ref(), &user, id).await?;

    let source = existing.source.clone();
    let target = body
        .target
        .clone()
        .unwrap_or_else(|| existing.target.clone());
    let schedule = body
        .schedule
        .clone()
        .unwrap_or_else(|| existing.schedule.clone());
    validate_job(&source, &target, &schedule)?;

    db::backup::update_job(
        pg_pool.get_ref(),
        id,
        body.name.as_deref(),
        body.target.as_deref(),
        body.database.as_deref(),
        body.schedule.as_deref(),
        body.retention_daily,
        body.retention_weekly,
        body.enabled,
    )
    .await
    .map_err(JsonResponse::internal_server_error)?;

    let updated = db::backup::fetch_job(pg_pool.get_ref(), id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Backup job not found"))?;
    Ok(JsonResponse::build()
        .set_item(job_response(&updated))
        .ok("Backup job updated"))
}

#[delete("/jobs/{id}")]
pub async fn delete_job(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.into_inner();
    fetch_owned_job(pg_pool.get_ref(), &user, id).await?;
    db::backup::delete_job(pg_pool.get_ref(), id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    Ok(JsonResponse::build()
        .set_item(json!({ "deleted": true }))
        .ok("Backup job deleted"))
}

// ── Runs ────────────────────────────────────────────────────────────────

#[get("/jobs/{id}/runs")]
pub async fn list_runs(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let job = fetch_owned_job(pg_pool.get_ref(), &user, path.into_inner()).await?;
    let runs = db::backup::list_runs_by_job(pg_pool.get_ref(), job.id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    let responses: Vec<RunResponse> = runs.iter().map(run_response).collect();
    Ok(JsonResponse::build().set_list(responses).ok("Backup runs"))
}

/// Trigger a backup run for a job: enqueue the `backup` command for the
/// project's agent. Returns the created run id + queued command id.
#[post("/jobs/{id}/run")]
pub async fn trigger_run(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let job = fetch_owned_job(pg_pool.get_ref(), &user, path.into_inner()).await?;
    let dest = db::backup::fetch_destination(pg_pool.get_ref(), job.destination_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Backup destination not found"))?;

    // Address the agent via the project's deployment.
    let deployments = db::deployment::fetch_by_project_id(pg_pool.get_ref(), job.project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    let deployment = deployments
        .into_iter()
        .next()
        .ok_or_else(|| JsonResponse::bad_request("Project has no deployment to back up"))?;

    // The OAuth2 token endpoint of the storage auth provider (not a secret).
    let token_url = std::env::var("BACKUP_TOKEN_URL").unwrap_or_default();
    if token_url.is_empty() {
        return Err(JsonResponse::internal_server_error(
            "backup token endpoint is not configured (BACKUP_TOKEN_URL)",
        ));
    }

    let (run_id, command_id) = crate::services::backup::enqueue_backup_run(
        pg_pool.get_ref(),
        deployment.id,
        &job,
        &dest,
        &token_url,
    )
    .await
    .map_err(JsonResponse::internal_server_error)?;

    Ok(JsonResponse::build()
        .set_item(json!({ "runId": run_id, "commandId": command_id }))
        .created("Backup run queued"))
}
