use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{delete, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;
use crate::db;
use crate::models::Project;

#[tracing::instrument(name = "Delete project of a user.")]
#[delete("/{id}")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// Get project apps of logged user only
    let (id,) = path.into_inner();

    let project = db::project::fetch(pg_pool.get_ref(), id).await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .map(|project| { project }).unwrap();
    //
    //         match project {
    //             Some(project) if project.user_id != user.id => {
    //                 Err(JsonResponse::<models::Project>::build().not_found("not found"))
    //             }
    //             Some(project) => {
    //
    //                     match db::project::delete(pg_pool.get_ref(), id)
    //                         .await
    //                         .map_err(|e| println!("{:?}", e))
    //                     {
    //                         Ok(true) => {
    //                             Ok(JsonResponse::build().set_item(Some(project)).ok("Deleted"))
    //                         }
    //                         _ => {
    //                             Err(JsonResponse::<models::Project>::build().bad_request("Could not delete"))
    //                         }
    //                     }
    //             },
    //             None => Err(JsonResponse::<models::Project>::build().not_found("not found")),
    //         }
    //     })


    Ok(JsonResponse::<Project>::build().ok("Deleted"))
}
