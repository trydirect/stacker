use crate::configuration::Settings;
use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use crate::routes::project::access::{self, Need};
use crate::routes::project::invitation::{require_team_plan, reserve_seat, INVITABLE_ROLES};
use actix_web::{delete, get, patch, post, web, HttpResponse, Responder, Result};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct AddProjectMemberRequest {
    pub user_id: String,
    pub role: String,
}

/// Members and invitations are the owner's: a member gets 403, anyone else 404.
async fn fetch_owned_project(
    pool: &PgPool,
    project_id: i32,
    user_id: &str,
) -> Result<models::Project, actix_web::Error> {
    access::require(pool, project_id, user_id, Need::Owner)
        .await
        .map(|(project, _)| project)
}

#[tracing::instrument(name = "Share project with member", skip_all)]
#[post("/{id}/members")]
pub async fn add(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    payload: web::Json<AddProjectMemberRequest>,
    pg_pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
) -> Result<impl Responder> {
    let project_id = path.0;

    let project = fetch_owned_project(pg_pool.get_ref(), project_id, &user.id).await?;

    if !INVITABLE_ROLES.contains(&payload.role.as_str()) {
        return Err(JsonResponse::<models::ProjectMember>::build()
            .bad_request("Only member and viewer roles are supported"));
    }
    if payload.user_id == project.user_id {
        return Err(JsonResponse::<models::ProjectMember>::build().conflict("cannot_invite_self"));
    }
    require_team_plan(settings.get_ref(), &user).await?;

    // Changing an existing member's role takes no new seat.
    let existing = db::project_member::fetch(pg_pool.get_ref(), project_id, &payload.user_id)
        .await
        .map_err(|err| JsonResponse::<models::ProjectMember>::build().internal_server_error(err))?;
    if existing.is_none() {
        // Insert under the seat lock so two requests cannot both take the
        // last seat; the upsert below then only settles the role.
        let mut tx = pg_pool
            .begin()
            .await
            .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;
        reserve_seat(&mut tx, project_id).await?;
        db::project_invitation::insert_member(
            &mut *tx,
            project_id,
            &payload.user_id,
            &payload.role,
            &user.id,
        )
        .await
        .map_err(JsonResponse::internal_server_error)?;
        tx.commit()
            .await
            .map_err(|err| JsonResponse::internal_server_error(err.to_string()))?;
    }

    let member = db::project_member::upsert(
        pg_pool.get_ref(),
        project_id,
        &payload.user_id,
        &payload.role,
        &user.id,
    )
    .await
    .map_err(|err| JsonResponse::<models::ProjectMember>::build().internal_server_error(err))?;

    Ok(JsonResponse::build().set_item(member).ok("OK"))
}

#[derive(Debug, Deserialize)]
pub struct ServerAccessForm {
    pub enabled: bool,
}

/// Owner only: whether members get the servers' SSH key and passwords in the
/// deployment report.
#[tracing::instrument(name = "Set members server access", skip_all)]
#[patch("/{id}/server-access")]
pub async fn set_server_access(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    web::Json(form): web::Json<ServerAccessForm>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let project = fetch_owned_project(pg_pool.get_ref(), path.0, &user.id).await?;
    db::project::set_members_server_access(pg_pool.get_ref(), project.id, form.enabled)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))?;
    Ok(JsonResponse::<models::Project>::build().ok("OK"))
}

#[tracing::instrument(name = "List project members", skip_all)]
#[get("/{id}/members")]
pub async fn list(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let project_id = path.0;

    let _project = fetch_owned_project(pg_pool.get_ref(), project_id, &user.id).await?;

    let members = db::project_member::fetch_by_project_with_email(pg_pool.get_ref(), project_id)
        .await
        .map_err(|err| JsonResponse::<models::ProjectMember>::build().internal_server_error(err))?;

    Ok(JsonResponse::build().set_list(members).ok("OK"))
}

#[tracing::instrument(name = "Delete project member", skip_all)]
#[delete("/{id}/members/{member_user_id}")]
pub async fn delete(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32, String)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let (project_id, member_user_id) = path.into_inner();

    let _project = fetch_owned_project(pg_pool.get_ref(), project_id, &user.id).await?;

    let deleted = db::project_member::delete(pg_pool.get_ref(), project_id, &member_user_id)
        .await
        .map_err(|err| JsonResponse::<models::ProjectMember>::build().internal_server_error(err))?;

    if !deleted {
        return Err(JsonResponse::<models::ProjectMember>::build().not_found("not found"));
    }

    Ok(HttpResponse::NoContent().finish())
}
