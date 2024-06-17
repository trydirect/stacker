use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{delete, web, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use crate::db;
use crate::models::Server;

#[tracing::instrument(name = "Delete user's server.")]
#[delete("/{id}")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    // Get server apps of logged user only
    let (id,) = path.into_inner();

    let server = db::server::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Server>::build().internal_server_error(err))
        .and_then(|server| {
            match server {
                Some(server) if server.user_id != user.id => {
                    Err(JsonResponse::<models::Server>::build().bad_request("Delete is forbidden"))
                }
                Some(server) => {
                    Ok(server)
                },
                None => Err(JsonResponse::<models::Server>::build().not_found(""))
            }
        })?;

    db::server::delete(pg_pool.get_ref(), server.id)
        .await
        .map_err(|err| JsonResponse::<Server>::build().internal_server_error(err))
        .and_then(|result| {
            match result
            {
                true => {
                    Ok(JsonResponse::<Server>::build().ok("Item deleted"))
                }
                _ => {
                    Err(JsonResponse::<Server>::build().bad_request("Could not delete"))
                }
            }
        })

}
