use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;

#[tracing::instrument(name = "Get logged user project.")]
#[get("/{id}")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// Get project apps of logged user only
    let (id,) = path.into_inner();

    db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .and_then(|project| match project {
            Some(project) if project.user_id != user.id => {
                Err(JsonResponse::<models::Project>::build().not_found("not found"))
            }
            Some(project) => Ok(JsonResponse::build().set_item(Some(project)).ok("OK")),
            None => Err(JsonResponse::<models::Project>::build().not_found("not found")),
        })
}

#[tracing::instrument(name = "Get user's project list.")]
#[get("/user/{id}")]
pub async fn list(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// This is admin endpoint, used by a client app, client app is confidential
    /// it should return projects by user id
    /// in order to pass validation at external deployment service
    let user_id = path.into_inner().0;

    db::project::fetch_by_user(pg_pool.get_ref(), &user_id)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .map(|projects| JsonResponse::build().set_list(projects).ok("OK"))
}
