use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{post, web, Responder, Result};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct AddProjectMemberRequest {
    pub user_id: String,
    pub role: String,
}

#[tracing::instrument(name = "Share project with member", skip_all)]
#[post("/{id}/members")]
pub async fn add(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    payload: web::Json<AddProjectMemberRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let project_id = path.0;

    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|err| JsonResponse::<models::ProjectMember>::build().internal_server_error(err))?
        .ok_or_else(|| JsonResponse::<models::ProjectMember>::build().not_found("not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::<models::ProjectMember>::build().not_found("not found"));
    }

    if payload.role != "viewer" {
        return Err(
            JsonResponse::<models::ProjectMember>::build().bad_request("Only viewer role is supported")
        );
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
