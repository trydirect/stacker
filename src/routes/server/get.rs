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
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
    db::server::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|_err| JsonResponse::<models::Server>::build()
            .internal_server_error(""))
        .and_then(|server| {
            match server {
                Some(project) if project.user_id != user.id => {
                    Err(JsonResponse::not_found("not found"))
                },
                Some(server) => Ok(JsonResponse::build().set_item(Some(server)).ok("OK")),
                None => Err(JsonResponse::not_found("not found")),
            }
        })

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
