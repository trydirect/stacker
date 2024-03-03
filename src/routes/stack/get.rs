use crate::db;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{get, web, Responder, Result};
use sqlx::PgPool;
use std::convert::From;
use std::sync::Arc;
use tracing::Instrument;

#[tracing::instrument(name = "User get user's stack list.")]
#[get("")]
pub async fn list(
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// This is admin endpoint, used by a client app, client app is confidential
    /// it should return stacks by user id
    /// in order to pass validation at external deployment service
    db::stack::fetch_by_user(pg_pool.get_ref(), &user.id)
        .await
        .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error(err))
        .map(|stacks| JsonResponse::build().set_list(stacks).ok("OK"))
}

#[tracing::instrument(name = "User get logged user stack.")]
#[get("/{id}")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// Get stack apps of logged user only
    let (id,) = path.into_inner();

    db::stack::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error(err))
        .and_then(|stack| match stack {
            Some(stack) if stack.user_id != user.id => {
                Err(JsonResponse::<models::Stack>::build().not_found("not found"))
            }
            Some(stack) => Ok(JsonResponse::build().set_item(Some(stack)).ok("OK")),
            None => Err(JsonResponse::<models::Stack>::build().not_found("not found")),
        })
}

#[tracing::instrument(name = "Admin get logged user stack.")]
#[get("/{id}")]
pub async fn admin_item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// Get stack apps of logged user only
    let (id,) = path.into_inner();

    db::stack::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error(err))
        .and_then(|stack| match stack {
            Some(stack) => Ok(JsonResponse::build().set_item(Some(stack)).ok("OK")),
            None => Err(JsonResponse::<models::Stack>::build().not_found("not found")),
        })
}

#[tracing::instrument(name = "User get user's stack list.")]
#[get("/user/{id}")]
pub async fn admin_list(
    admin: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// This is admin endpoint, used by a client app, client app is confidential
    /// it should return stacks by user id
    /// in order to pass validation at external deployment service
    let user_id = path.into_inner().0;

    db::stack::fetch_by_user(pg_pool.get_ref(), &user_id)
        .await
        .map_err(|err| JsonResponse::<models::Stack>::build().internal_server_error(err))
        .map(|stacks| JsonResponse::build().set_list(stacks).ok("OK"))
}
