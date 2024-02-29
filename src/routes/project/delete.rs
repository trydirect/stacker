use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{delete, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;

#[tracing::instrument(name = "Get logged user project.")]
#[delete("/{id}")]
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
            Some(project) => {
                db::project::delete(pg_pool.get_ref(), id);
                Ok(JsonResponse::build().set_item(Some(project)).ok("Deleted"))
            },
            None => Err(JsonResponse::<models::Project>::build().not_found("not found")),
        })

}
