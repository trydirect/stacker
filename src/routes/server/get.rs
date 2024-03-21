use std::sync::Arc;
use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
// use tracing::Instrument;

// workflow
// add, update, list, get(user_id), ACL,
// ACL - access to func for a user
// ACL - access to objects for a user

#[tracing::instrument(name = "Get server.")]
#[get("/{id}")]
pub async fn item(
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
    let server = db::server::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|_err| JsonResponse::<models::Server>::build()
            .internal_server_error(""))
        .and_then(|server| {
            match server {
                Some(server) => { Ok(server) },
                None => Err(JsonResponse::<models::Server>::build().not_found("object not found"))
            }
        })?;

    Ok(JsonResponse::build().set_item(server).ok("OK"))
}

#[tracing::instrument(name = "Get all servers.")]
#[get("")]
pub async fn list(
    path: web::Path<()>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    db::server::fetch_by_user(pg_pool.get_ref(), user.id.as_ref())
        .await
        .map(|server| JsonResponse::build().set_list(server).ok("OK"))
        .map_err(|_err| JsonResponse::<models::Server>::build().internal_server_error(""))
}
