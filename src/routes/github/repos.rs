//! Repo Connect API (WS-G G4): bind GitHub repositories to projects.
//!
//! The `autodeploy` switch lives here (`github_repo.autodeploy`); the old
//! catalog form field `autodeploy` (forms/project/app.rs) is superseded by it.

use crate::db;
use crate::forms::{CreateGithubRepoRequest, GithubRepoResponse, UpdateGithubRepoRequest};
use crate::helpers::JsonResponse;
use crate::models;
use crate::routes::project::access::{self, Need};
use actix_web::{delete, get, patch, post, web, Responder, Result};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

async fn check_project_owner(
    pool: &PgPool,
    user: &models::User,
    project_id: i32,
) -> Result<(), actix_web::Error> {
    let project = db::project::fetch(pool, project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;
    access::check(pool, &project, &user.id, Need::Owner, "Project not found")
        .await
        .map(|_| ())
}

fn validate_repo_name(repo: &str) -> Result<(), actix_web::Error> {
    if repo.matches('/').count() != 1 || repo.starts_with('/') || repo.ends_with('/') {
        return Err(JsonResponse::bad_request(
            "repo must be in 'owner/name' form",
        ));
    }
    Ok(())
}

#[tracing::instrument(name = "Connect GitHub repo", skip_all)]
#[post("/repos")]
pub async fn create_handler(
    user: web::ReqData<Arc<models::User>>,
    body: web::Json<CreateGithubRepoRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    validate_repo_name(&body.repo)?;
    check_project_owner(pg_pool.get_ref(), &user, body.project_id).await?;

    let repo = db::github::insert_repo(
        pg_pool.get_ref(),
        &user.id,
        body.installation_id,
        Some(body.project_id),
        &body.repo,
        &body.branch,
        &body.watch_paths,
        body.autodeploy,
    )
    .await
    .map_err(|error| {
        if error.contains("duplicate key") {
            JsonResponse::<String>::build()
                .conflict("This repo and branch are already connected")
        } else {
            JsonResponse::internal_server_error(error)
        }
    })?;

    Ok(JsonResponse::build()
        .set_item(GithubRepoResponse::from(repo))
        .created("Repository connected"))
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(rename = "projectId")]
    pub project_id: Option<i32>,
}

#[tracing::instrument(name = "List GitHub repos", skip_all)]
#[get("/repos")]
pub async fn list_handler(
    user: web::ReqData<Arc<models::User>>,
    query: web::Query<ListQuery>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let repos = db::github::list_repos(pg_pool.get_ref(), &user.id, query.project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .into_iter()
        .map(GithubRepoResponse::from)
        .collect::<Vec<_>>();

    Ok(JsonResponse::build()
        .set_list(repos)
        .ok("Connected repositories"))
}

/// Repos bound to a project are readable/manageable by its owner only.
async fn fetch_owned_repo(
    pool: &PgPool,
    user: &models::User,
    repo_id: i32,
) -> Result<models::GithubRepo, actix_web::Error> {
    let repo = db::github::fetch_repo(pool, repo_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("Repository connection not found"))?;

    match repo.project_id {
        Some(project_id) => check_project_owner(pool, user, project_id).await?,
        None if repo.user_id == user.id => {}
        None => return Err(JsonResponse::not_found("Repository connection not found")),
    }
    Ok(repo)
}

#[tracing::instrument(name = "Update GitHub repo", skip_all)]
#[patch("/repos/{id}")]
pub async fn update_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    body: web::Json<UpdateGithubRepoRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let repo = fetch_owned_repo(pg_pool.get_ref(), &user, path.into_inner()).await?;

    let updated = db::github::update_repo(
        pg_pool.get_ref(),
        repo.id,
        body.branch.as_deref(),
        body.watch_paths.as_deref(),
        body.autodeploy,
    )
    .await
    .map_err(|error| {
        if error.contains("duplicate key") {
            JsonResponse::<String>::build()
                .conflict("This repo and branch are already connected")
        } else {
            JsonResponse::internal_server_error(error)
        }
    })?;

    Ok(JsonResponse::build()
        .set_item(GithubRepoResponse::from(updated))
        .ok("Repository connection updated"))
}

#[tracing::instrument(name = "Disconnect GitHub repo", skip_all)]
#[delete("/repos/{id}")]
pub async fn delete_handler(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let repo = fetch_owned_repo(pg_pool.get_ref(), &user, path.into_inner()).await?;
    db::github::delete_repo(pg_pool.get_ref(), repo.id)
        .await
        .map_err(JsonResponse::internal_server_error)?;

    Ok(JsonResponse::build()
        .set_item(json!({ "deleted": true }))
        .ok("Repository disconnected"))
}
